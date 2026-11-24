use std::{cell::Cell, time::Duration};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, Client, ClientId, CsrfToken, EndpointNotSet, EndpointSet,
    ExtraTokenFields, PkceCodeChallenge, RedirectUrl, RequestTokenError, Scope,
    StandardTokenResponse, SyncHttpClient, TokenResponse, TokenUrl,
    basic::{
        BasicErrorResponse, BasicErrorResponseType, BasicRevocationErrorResponse, BasicTokenType,
    },
    reqwest::{self, blocking::Client as HttpClient},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::Url;

use super::{CALLBACK, Error, PendingLogin, Session, unix_time};

pub(super) const UGM_ID: &str = "https://oauth.simaster.ugm.ac.id";
const ISSUER: &str = "oauth.simaster.ugm.ac.id";
const SCOPES: [&str; 2] = ["openid", "user.read"];

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    code_challenge_methods_supported: Vec<String>,
    id_token_signing_alg_values_supported: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct OidcFields {
    id_token: String,
}

impl ExtraTokenFields for OidcFields {}

pub(super) type OidcClient = Client<
    BasicErrorResponse,
    StandardTokenResponse<OidcFields, BasicTokenType>,
    oauth2::basic::BasicTokenIntrospectionResponse,
    oauth2::StandardRevocableToken,
    BasicRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

pub(crate) fn begin_login(client_id: &str, origin: &str) -> Result<PendingLogin, Error> {
    if client_id.trim().is_empty() {
        return Err(Error::ClientId);
    }
    let http = HttpClient::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| Error::Network)?;
    let discovery: Discovery = fetch(&http, &format!("{origin}/.well-known/openid-configuration"))?;
    if discovery.issuer != ISSUER
        || !discovery
            .code_challenge_methods_supported
            .iter()
            .any(|method| method == "S256")
        || !discovery
            .id_token_signing_alg_values_supported
            .iter()
            .any(|alg| alg == "RS256")
    {
        return Err(Error::Upstream("unexpected UGM ID discovery metadata"));
    }

    let authorize = endpoint(
        origin,
        &discovery.authorization_endpoint,
        "/oauth/authorize",
    )?;
    let token = endpoint(origin, &discovery.token_endpoint, "/oauth/token")?;
    let jwks_url = endpoint(origin, &discovery.jwks_uri, "/.well-known/jwks.json")?;
    let client: OidcClient = Client::new(ClientId::new(client_id.to_owned()))
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::from_url(authorize))
        .set_token_uri(TokenUrl::from_url(token))
        .set_redirect_uri(RedirectUrl::from_url(
            Url::parse(CALLBACK).expect("fixed loopback URL"),
        ));
    let nonce = CsrfToken::new_random();
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(SCOPES.map(|scope| Scope::new(scope.to_owned())))
        .add_extra_param("nonce", nonce.secret())
        .set_pkce_challenge(challenge)
        .url();
    Ok(PendingLogin {
        url,
        state,
        nonce,
        verifier,
        client,
        http,
        jwks_url,
    })
}

fn endpoint(origin: &str, advertised: &str, path: &str) -> Result<Url, Error> {
    let expected = format!("{origin}{path}");
    if advertised != expected && advertised != format!("{ISSUER}{path}") {
        return Err(Error::Upstream("unexpected UGM ID endpoint"));
    }
    Url::parse(&expected).map_err(|_| Error::Upstream("invalid UGM ID endpoint"))
}

pub(super) fn exchange(pending: PendingLogin, code: String) -> Result<Session, Error> {
    let status = Cell::new(None);
    let send = |request| {
        let response = pending.http.call(request)?;
        status.set(Some(response.status().as_u16()));
        Ok::<_, <HttpClient as SyncHttpClient>::Error>(response)
    };
    let tokens = pending
        .client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pending.verifier)
        .request(&send)
        .map_err(|error| match (status.get(), error) {
            (Some(429), _) => Error::RateLimited,
            (_, RequestTokenError::Request(_)) => Error::Network,
            (_, RequestTokenError::ServerResponse(response))
                if *response.error() == BasicErrorResponseType::InvalidGrant =>
            {
                Error::Failed("UGM ID rejected the authorization code")
            }
            _ => Error::Upstream("unexpected UGM ID token response"),
        })?;
    let keys = fetch(&pending.http, pending.jwks_url.as_str())?;
    let id_token = &tokens.extra_fields().id_token;
    let client_id = pending.client.client_id().as_str();
    let claims = verify(id_token, &keys, client_id, &pending.nonce)?;
    let lifetime = tokens
        .expires_in()
        .ok_or(Error::Upstream("missing access-token lifetime"))?;
    let expires_at_unix = unix_time()
        .checked_add(lifetime)
        .ok_or(Error::Upstream("invalid access-token lifetime"))?
        .as_secs();
    let scopes = tokens.scopes().map_or_else(
        || SCOPES.map(str::to_owned).to_vec(),
        |scopes| {
            scopes
                .iter()
                .map(|scope| scope.as_ref().to_owned())
                .collect()
        },
    );
    Ok(Session {
        access_token: tokens.access_token().secret().to_owned(),
        subject: claims.sub,
        username: claims.preferred_username,
        scopes,
        expires_at_unix,
    })
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    nonce: String,
    aud: Audience,
    azp: Option<String>,
    preferred_username: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    Single(String),
    Multiple(Vec<String>),
}

fn verify(token: &str, keys: &JwkSet, client_id: &str, nonce: &CsrfToken) -> Result<Claims, Error> {
    let header = jsonwebtoken::decode_header(token)
        .map_err(|_| Error::Upstream("invalid ID-token header"))?;
    let key = header
        .kid
        .as_deref()
        .and_then(|kid| keys.find(kid))
        .ok_or(Error::Upstream("unknown ID-token signing key"))?;
    let key =
        DecodingKey::from_jwk(key).map_err(|_| Error::Upstream("invalid ID-token signing key"))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["iss", "aud", "exp", "sub"]);
    validation.leeway = 0;
    let claims = jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map_err(|_| Error::Upstream("invalid UGM ID token"))?
        .claims;
    let audience_matches = match &claims.aud {
        Audience::Single(audience) => audience == client_id,
        Audience::Multiple(audiences) => audiences == &[client_id],
    };
    if !audience_matches
        || claims.azp.as_deref().is_some_and(|azp| azp != client_id)
        || CsrfToken::new(claims.nonce.clone()) != *nonce
        || claims.sub.is_empty()
    {
        return Err(Error::Upstream("invalid UGM ID token claims"));
    }
    Ok(claims)
}

fn fetch<T: DeserializeOwned>(http: &HttpClient, url: &str) -> Result<T, Error> {
    let response = http.get(url).send().map_err(|_| Error::Network)?;
    match response.status().as_u16() {
        200 => (),
        429 => return Err(Error::RateLimited),
        _ => return Err(Error::Upstream("unexpected UGM ID endpoint response")),
    }
    let body = response.bytes().map_err(|_| Error::Network)?;
    serde_json::from_slice(&body).map_err(|_| Error::Upstream("invalid UGM ID endpoint response"))
}
