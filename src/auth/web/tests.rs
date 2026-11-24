use std::{collections::HashMap, thread};

use tiny_http::{Header, Method, Request, Response, Server};

use super::Web;

fn respond(request: Request, status: u16, body: &str, headers: &[(&str, &str)]) {
    let mut response = Response::from_string(body).with_status_code(status);
    for (key, value) in headers {
        response.add_header(Header::from_bytes(*key, *value).unwrap());
    }
    request.respond(response).unwrap();
}

fn next(server: &Server, method: Method, path: &str) -> Request {
    let request = server
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(*request.method(), method);
    assert_eq!(request.url().split('?').next(), Some(path));
    request
}

fn form(request: &mut Request) -> HashMap<String, String> {
    let mut body = Vec::new();
    request.as_reader().read_to_end(&mut body).unwrap();
    url::form_urlencoded::parse(&body).into_owned().collect()
}

fn mock_entry() -> keyring::Entry {
    keyring::Entry::new_with_credential(Box::<keyring::mock::MockCredential>::default())
}

#[test]
fn web_login_and_reuse_through_direct_or_human_captcha() {
    for needs_challenge in [false, true] {
        let server = Server::http(("127.0.0.1", 0)).unwrap();
        let origin = format!("http://{}", server.server_addr());
        let mut web = Web::new(&origin, &origin, mock_entry()).unwrap();

        let portal = thread::spawn(move || {
            let mut direct = next(&server, Method::Post, "/services/simaster/service_login");
            let fields = form(&mut direct);
            assert_eq!(fields["username"], "mock-student");
            assert_eq!(fields["password"], "mock-password");
            let session = ("Set-Cookie", "simaster-ugm_sess=mock-cookie; Path=/");
            if needs_challenge {
                respond(direct, 200, r#"{"isLogin":0}"#, &[]);
                let start = next(&server, Method::Get, super::SIGNIN);
                respond(
                    start,
                    302,
                    "",
                    &[("Location", "/cas/login?service=simaster")],
                );
                let login = next(&server, Method::Get, "/cas/login");
                respond(
                    login,
                    200,
                    r#"<input type="hidden" name="lt" value="mock-lt">"#,
                    &[],
                );
                let mut submit = next(&server, Method::Post, "/cas/login");
                assert_eq!(form(&mut submit)["lt"], "mock-lt");
                respond(submit, 302, "", &[("Location", super::CAPTCHA)]);
                let challenge = next(&server, Method::Get, super::CAPTCHA);
                respond(
                    challenge,
                    200,
                    r#"<input type="hidden" name="simasterUGM_token" value="mock-csrf">"#,
                    &[],
                );
                let asset = next(&server, Method::Get, super::CAPTCHA_ASSET);
                respond(asset, 200, "mock-audio", &[("Content-Type", "audio/wav")]);
                let mut verify = next(&server, Method::Post, super::CAPTCHA);
                let submitted = form(&mut verify);
                assert_eq!(submitted["captcha"], "human-answer");
                assert_eq!(submitted["simasterUGM_token"], "mock-csrf");
                respond(verify, 302, "", &[("Location", super::DASHBOARD), session]);
                respond(
                    next(&server, Method::Get, super::DASHBOARD),
                    200,
                    "Dashboard",
                    &[],
                );
            } else {
                respond(direct, 200, r#"{"isLogin":1}"#, &[session]);
            }
            for _ in 0..2 {
                let dashboard = next(&server, Method::Get, super::DASHBOARD);
                assert!(dashboard.headers().iter().any(|header| {
                    header.field.equiv("Cookie")
                        && header.value.as_str() == "simaster-ugm_sess=mock-cookie"
                }));
                respond(dashboard, 200, "Dashboard", &[]);
            }
        });
        web.login("mock-student", "mock-password", |challenge| {
            assert!(needs_challenge);
            assert_eq!(challenge.extension, ".wav");
            assert_eq!(challenge.bytes, b"mock-audio");
            assert!(!challenge.rejected);
            Ok("human-answer".to_owned())
        })
        .unwrap();
        web.jar.clear();
        assert_eq!(web.reuse(Some("other-user")).unwrap(), None);
        assert_eq!(web.reuse(None).unwrap().as_deref(), Some("mock-student"));
        assert!(web.logout().unwrap());
        assert_eq!(web.status().unwrap(), None);
        portal.join().unwrap();
    }
}

#[test]
fn rejected_captcha_retries_with_fresh_token_and_escalates() {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let origin = format!("http://{}", server.server_addr());
    let mut web = Web::new(&origin, &origin, mock_entry()).unwrap();

    let portal = thread::spawn(move || {
        respond(
            next(&server, Method::Post, "/services/simaster/service_login"),
            200,
            r#"{"isLogin":0}"#,
            &[],
        );
        respond(
            next(&server, Method::Get, super::SIGNIN),
            302,
            "",
            &[("Location", "/cas/login?service=simaster")],
        );
        respond(
            next(&server, Method::Get, "/cas/login"),
            200,
            r#"<input type="hidden" name="lt" value="mock-lt">"#,
            &[],
        );
        respond(
            next(&server, Method::Post, "/cas/login"),
            302,
            "",
            &[("Location", super::CAPTCHA)],
        );
        let session = ("Set-Cookie", "simaster-ugm_sess=mock-cookie; Path=/");
        for (token, expected, rejected) in [
            ("csrf-1", "ocr-answer", false),
            ("csrf-2", "human-answer", true),
        ] {
            respond(
                next(&server, Method::Get, super::CAPTCHA),
                200,
                &format!(r#"<input type="hidden" name="simasterUGM_token" value="{token}">"#),
                &[],
            );
            let asset = next(&server, Method::Get, super::CAPTCHA_ASSET);
            assert!(
                asset.url().contains("?_"),
                "asset fetch must be cache-busted"
            );
            respond(asset, 200, "mock-audio", &[("Content-Type", "audio/wav")]);
            let mut verify = next(&server, Method::Post, super::CAPTCHA);
            let submitted = form(&mut verify);
            assert_eq!(submitted["simasterUGM_token"], token);
            assert_eq!(submitted["captcha"], expected);
            if !rejected {
                // First answer rejected: re-present the challenge.
                respond(verify, 302, "", &[("Location", super::CAPTCHA)]);
            } else {
                respond(verify, 302, "", &[("Location", super::DASHBOARD), session]);
                respond(
                    next(&server, Method::Get, super::DASHBOARD),
                    200,
                    "Dashboard",
                    &[],
                );
            }
        }
        for _ in 0..2 {
            let dashboard = next(&server, Method::Get, super::DASHBOARD);
            assert!(dashboard.headers().iter().any(|header| {
                header.field.equiv("Cookie")
                    && header.value.as_str() == "simaster-ugm_sess=mock-cookie"
            }));
            respond(dashboard, 200, "Dashboard", &[]);
        }
    });
    web.login("mock-student", "mock-password", |challenge| {
        Ok(if challenge.rejected {
            "human-answer".to_owned()
        } else {
            "ocr-answer".to_owned()
        })
    })
    .unwrap();
    web.jar.clear();
    assert_eq!(web.reuse(None).unwrap().as_deref(), Some("mock-student"));
    assert!(web.logout().unwrap());
    portal.join().unwrap();
}

#[test]
fn captcha_session_lost_fails_the_login() {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let origin = format!("http://{}", server.server_addr());
    let mut web = Web::new(&origin, &origin, mock_entry()).unwrap();

    let portal = thread::spawn(move || {
        respond(
            next(&server, Method::Post, "/services/simaster/service_login"),
            200,
            r#"{"isLogin":0}"#,
            &[],
        );
        respond(
            next(&server, Method::Get, super::SIGNIN),
            302,
            "",
            &[("Location", "/cas/login?service=simaster")],
        );
        respond(
            next(&server, Method::Get, "/cas/login"),
            200,
            r#"<input type="hidden" name="lt" value="mock-lt">"#,
            &[],
        );
        respond(
            next(&server, Method::Post, "/cas/login"),
            302,
            "",
            &[("Location", super::CAPTCHA)],
        );
        respond(
            next(&server, Method::Get, super::CAPTCHA),
            200,
            r#"<input type="hidden" name="simasterUGM_token" value="mock-csrf">"#,
            &[],
        );
        respond(
            next(&server, Method::Get, super::CAPTCHA_ASSET),
            200,
            "mock-audio",
            &[("Content-Type", "audio/wav")],
        );
        // A lost session bounces the verify back to the login service.
        respond(
            next(&server, Method::Post, super::CAPTCHA),
            302,
            "",
            &[("Location", "/services/simaster/service_login")],
        );
        respond(
            next(&server, Method::Get, "/services/simaster/service_login"),
            200,
            r#"{"isLogin":0}"#,
            &[],
        );
    });
    let result = web.login("mock-student", "mock-password", |_| {
        Ok("ocr-answer".to_owned())
    });
    portal.join().unwrap();
    assert!(matches!(
        result,
        Err(super::Error::Failed(
            "SIMASTER lost the login session during CAPTCHA verification"
        ))
    ));
}

#[test]
fn rejected_cas_credentials_fail_as_auth_not_browser_required() {
    let portal = Server::http(("127.0.0.1", 0)).unwrap();
    let cas = Server::http(("127.0.0.1", 0)).unwrap();
    let (portal_origin, cas_origin) = (
        format!("http://{}", portal.server_addr()),
        format!("http://{}", cas.server_addr()),
    );
    let mut web = Web::new(&portal_origin, &cas_origin, mock_entry()).unwrap();

    let portal_thread = thread::spawn(move || {
        respond(
            next(&portal, Method::Post, "/services/simaster/service_login"),
            200,
            r#"{"isLogin":0}"#,
            &[],
        );
        let start = next(&portal, Method::Get, super::SIGNIN);
        respond(
            start,
            302,
            "",
            &[(
                "Location",
                &format!("{cas_origin}/cas/login?service=simaster"),
            )],
        );
    });
    let cas_thread = thread::spawn(move || {
        respond(
            next(&cas, Method::Get, "/cas/login"),
            200,
            r#"<input type="hidden" name="lt" value="mock-lt">"#,
            &[],
        );
        // CAS re-renders its sign-in form for rejected credentials.
        respond(
            next(&cas, Method::Post, "/cas/login"),
            200,
            r#"<input type="hidden" name="lt" value="mock-lt">"#,
            &[],
        );
    });
    let result = web.login(
        "mock-student",
        "wrong-password",
        |_| Ok("unused".to_owned()),
    );
    portal_thread.join().unwrap();
    cas_thread.join().unwrap();
    assert!(matches!(
        result,
        Err(super::Error::Failed(
            "UGM CAS returned the sign-in page; check the username and password"
        ))
    ));
}

#[test]
fn ocr_rejects_a_model_that_does_not_match_the_pinned_checksum() {
    assert_eq!(super::ocr::verify(b"not the model"), None);
}

#[test]
fn ocr_classifies_rendered_digits_when_the_model_is_cached() {
    let Some(path) = super::ocr::cache_path().filter(|path| path.is_file()) else {
        eprintln!("skipping: run once with network to cache the CAPTCHA model");
        return;
    };
    assert!(
        super::ocr::read_verified(&path).is_some(),
        "cached model is corrupt"
    );
    let bytes = include_bytes!("../../../tests/fixtures/captcha-digits-1234.png");
    assert_eq!(super::ocr::classify(bytes).as_deref(), Some("1234"));
}
