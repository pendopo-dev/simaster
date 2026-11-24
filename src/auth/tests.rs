use std::{collections::HashMap, sync::mpsc, thread, time::Duration};
use tiny_http::{Method, Response, Server};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use oauth2::{PkceCodeChallenge, PkceCodeVerifier};
use serde_json::{Value, json};

use super::{CALLBACK, SessionStore, oidc};

const ISSUER: &str = "oauth.simaster.ugm.ac.id";

fn reply(request: tiny_http::Request, body: impl Into<String>) {
    let content_type = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
    request
        .respond(Response::from_string(body).with_header(content_type))
        .unwrap();
}

#[test]
fn browser_login_against_mock_provider() {
    let bad_claims = ["nonce", "iss", "aud", "aud-extra", "exp", "sub"];
    for rejection in ["", "state", "kid", "signature"]
        .into_iter()
        .chain(bad_claims)
    {
        let provider = Server::http(("127.0.0.1", 0)).unwrap();
        let provider_origin = format!("http://{}", provider.server_addr());
        let (send, receive) = mpsc::channel::<HashMap<String, String>>();
        let provider_thread = thread::spawn(move || {
            let discovery = provider.recv().unwrap();
            assert_eq!(discovery.url(), "/.well-known/openid-configuration");
            reply(
                discovery,
                include_str!("../../tests/fixtures/mock-oidc-discovery.json"),
            );
            let token = provider
                .recv_timeout(if rejection == "state" {
                    Duration::from_millis(350)
                } else {
                    Duration::from_secs(5)
                })
                .unwrap();
            if rejection == "state" {
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
                ("redirect_uri", CALLBACK),
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
            let key =
                EncodingKey::from_rsa_der(include_bytes!("../../tests/fixtures/mock-oidc-key.der"));
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
                include_str!("../../tests/fixtures/mock-oidc-jwks.json"),
            );
        });

        let pending = oidc::begin_login("mock-client", &provider_origin).unwrap();
        let mock = Box::<keyring::mock::MockCredential>::default();
        let store = SessionStore(keyring::Entry::new_with_credential(mock));
        let params: HashMap<_, _> = pending
            .authorization_url()
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["scope"], "openid user.read");
        assert_eq!(params["redirect_uri"], CALLBACK);
        let state = if rejection == "state" {
            "wrong-state"
        } else {
            &params["state"]
        }
        .to_owned();
        send.send(params).unwrap();
        let target = format!("/callback?code=mock-code&state={state}");
        let result = pending.complete(&target, &store);
        provider_thread.join().unwrap();
        if rejection.is_empty() {
            assert_eq!(result.unwrap().username.as_deref(), Some("mock-student"));
            let stored = serde_json::to_value(store.status().unwrap()).unwrap();
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
