use std::{collections::HashMap, sync::mpsc, thread};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use oauth2::{PkceCodeChallenge, PkceCodeVerifier, reqwest::blocking::Client};
use serde_json::{Value, json};
use url::Url;

use super::*;

const ISSUER: &str = "oauth.simaster.ugm.ac.id";

fn reply(request: tiny_http::Request, body: impl Into<String>) {
    let content_type = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
    request
        .respond(Response::from_string(body).with_header(content_type))
        .unwrap();
}

fn serve() -> Server {
    Server::http(("127.0.0.1", 0)).unwrap()
}

fn origin(server: &Server) -> String {
    format!("http://{}", server.server_addr())
}

#[test]
fn browser_login_against_mock_provider() {
    let claims = ["nonce", "iss", "aud", "aud-extra", "exp", "sub"];
    let flow = ["", "state", "kid", "signature", "timeout"];
    for rejection in flow.into_iter().chain(claims) {
        let (provider, callback) = (serve(), serve());
        let (provider_origin, callback_origin) = (origin(&provider), origin(&callback));
        let (send, receive) = mpsc::channel::<HashMap<String, String>>();
        let provider_thread = thread::spawn(move || {
            let discovery = provider.recv().unwrap();
            assert_eq!(discovery.url(), "/.well-known/openid-configuration");
            reply(
                discovery,
                include_str!("../../../tests/fixtures/mock-oidc-discovery.json"),
            );
            let token = provider
                .recv_timeout(if matches!(rejection, "state" | "timeout") {
                    Duration::from_millis(350)
                } else {
                    Duration::from_secs(5)
                })
                .unwrap();
            if matches!(rejection, "state" | "timeout") {
                assert!(
                    token.is_none(),
                    "{rejection} must prevent the token exchange"
                );
                return;
            }
            let mut token = token.unwrap();
            assert_eq!(token.url(), "/oauth/token");
            assert_eq!(*token.method(), Method::Post);
            let mut body = String::new();
            token.as_reader().read_to_string(&mut body).unwrap();
            let form: HashMap<_, _> = url::form_urlencoded::parse(body.as_bytes()).collect();
            let auth = receive.recv().unwrap();
            let expected = [
                ("grant_type", "authorization_code"),
                ("code", "mock-code"),
                ("client_id", "mock-client"),
                ("redirect_uri", auth::CALLBACK),
            ];
            for (key, value) in expected {
                assert_eq!(form[key], value);
            }
            assert!(!form.contains_key("client_secret"));
            let verifier = PkceCodeVerifier::new(form["code_verifier"].to_string());
            assert_eq!(
                PkceCodeChallenge::from_code_verifier_sha256(&verifier).as_str(),
                auth["code_challenge"]
            );

            let mut claims = json!({"sub":"student-1","iss":ISSUER,"aud":"mock-client",
                "exp":jsonwebtoken::get_current_timestamp() + 60,"nonce":auth["nonce"],
                "preferred_username":"mock-student"});
            match rejection {
                "iss" => claims["iss"] = json!(format!("https://{ISSUER}")),
                "aud" => claims["aud"] = json!("another-client"),
                "aud-extra" => claims["aud"] = json!(["mock-client", "another-client"]),
                "exp" => claims["exp"] = json!(1),
                "sub" => {
                    claims.as_object_mut().unwrap().remove("sub");
                }
                "nonce" => claims["nonce"] = json!("wrong-nonce"),
                _ => (),
            }
            let mut header = Header::new(Algorithm::RS256);
            let key_id = if rejection == "kid" {
                "unknown"
            } else {
                "fixture"
            };
            header.kid = Some(key_id.to_owned());
            let der = include_bytes!("../../../tests/fixtures/mock-oidc-key.der");
            let key = EncodingKey::from_rsa_der(der);
            let signed = jsonwebtoken::encode(&header, &claims, &key).unwrap();
            let jwt = if rejection == "signature" {
                format!("{}.AAAAAAAA", signed.rsplit_once('.').unwrap().0)
            } else {
                signed
            };
            reply(
                token,
                json!({"access_token":"mock-access-token","token_type":"Bearer",
                "expires_in":60,"scope":"openid user.read","id_token":jwt})
                .to_string(),
            );
            let jwks = provider
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(jwks.url(), "/.well-known/jwks.json");
            reply(
                jwks,
                include_str!("../../../tests/fixtures/mock-oidc-jwks.json"),
            );
        });

        let pending = auth::oidc::begin_login("mock-client", &provider_origin).unwrap();
        let mock = Box::<keyring::mock::MockCredential>::default();
        let store = SessionStore(keyring::Entry::new_with_credential(mock));
        let timeout = if rejection == "timeout" {
            Duration::ZERO
        } else {
            Duration::from_secs(5)
        };
        let result = browser_login(pending, &store, callback, timeout, |url| {
            if rejection == "timeout" {
                return false;
            }
            let url = Url::parse(url).unwrap();
            assert_eq!(url.path(), "/oauth/authorize");
            let params: HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(params["code_challenge_method"], "S256");
            assert_eq!(params["scope"], "openid user.read");
            assert_eq!(params["redirect_uri"], auth::CALLBACK);
            let state = if rejection == "state" {
                "wrong-state"
            } else {
                &params["state"]
            }
            .to_owned();
            send.send(params).unwrap();
            let request_url = format!("{callback_origin}/callback?code=mock-code&state={state}");
            thread::spawn(move || Client::new().get(request_url).send());
            rejection != "state"
        });
        provider_thread.join().unwrap();
        if rejection.is_empty() {
            let status = result.unwrap();
            assert_eq!(status.username.as_deref(), Some("mock-student"));
            print_status(Some(status), "ugm-id", true).unwrap();
            let stored = serde_json::to_value(store.status().unwrap()).unwrap();
            assert_eq!(stored["subject"], "student-1");
            assert_eq!(stored["expired"], false);
            assert_eq!(stored["scopes"], json!(["openid", "user.read"]));
            assert_eq!(stored["access_token"], Value::Null);
            assert!(store.logout().unwrap());
            assert!(!store.logout().unwrap());
        } else {
            assert!(result.is_err(), "unexpected success for {rejection}");
        }
        assert!(store.status().unwrap().is_none());
    }
}
