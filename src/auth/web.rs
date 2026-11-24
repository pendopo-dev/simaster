//! SIMASTER web sessions with OCR-assisted challenge completion.

use std::time::Duration;

use cookie_net::{Jar, RequestUrl};
use oauth2::reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder, Response},
    header::{CONTENT_TYPE, COOKIE as COOKIE_HEADER, LOCATION, SET_COOKIE},
    redirect::Policy,
};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{Error, unix_time};

const PORTAL: &str = "https://simaster.ugm.ac.id";
const CAS: &str = "https://sso.ugm.ac.id";
const SIGNIN: &str = "/ugmfw/signin_simaster/signin_proses";
const CAPTCHA: &str = "/ugmfw/signin_simaster/captchasound_verification";
const DASHBOARD: &str = "/beranda/beranda/view";
const COOKIE: &str = "simaster-ugm_sess";
const CAPTCHA_ASSET: &str = "/ugmfw/signin_simaster/captcha_sound/";
/// Fresh challenges per login, from the prior-art client.
const MAX_CAPTCHA_ATTEMPTS: u8 = 5;

pub struct Challenge {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
    /// True when the previous answer in this login was rejected and this is
    /// a re-presented challenge.
    pub rejected: bool,
}

pub struct Web {
    http: Client,
    jar: Jar,
    portal: Url,
    cas: Url,
    store: keyring::Entry,
}

#[derive(Serialize, Deserialize)]
struct Session {
    username: String,
    cookie: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LoginResult {
    Boolean(bool),
    Number(u8),
}

#[derive(Deserialize)]
struct DirectLogin {
    #[serde(rename = "isLogin")]
    accepted: LoginResult,
}

impl Web {
    pub fn open() -> Result<Self, Error> {
        let store = keyring::Entry::new("simaster", "web").map_err(|_| Error::Storage)?;
        Self::new(PORTAL, CAS, store)
    }

    fn new(portal: &str, cas: &str, store: keyring::Entry) -> Result<Self, Error> {
        // SSO only negotiates TLS 1.2 with CBC suites, which rustls cannot do,
        // so the web client uses the platform TLS stack.
        let http = Client::builder()
            .use_native_tls()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| Error::Network)?;
        Ok(Self {
            http,
            jar: Jar::new(),
            portal: Url::parse(portal).map_err(|_| Error::Upstream("invalid portal origin"))?,
            cas: Url::parse(cas).map_err(|_| Error::Upstream("invalid CAS origin"))?,
            store,
        })
    }

    fn stored(&self) -> Result<Option<Session>, Error> {
        match self.store.get_secret() {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| Error::Storage),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::Storage),
        }
    }

    /// Read local session metadata without contacting the portal.
    pub fn status(&self) -> Result<Option<String>, Error> {
        Ok(self.stored()?.map(|session| session.username))
    }

    pub fn logout(&mut self) -> Result<bool, Error> {
        self.jar.clear();
        match self.store.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(_) => Err(Error::Storage),
        }
    }

    /// Check the saved session before collecting credentials.
    pub fn reuse(&mut self, username: Option<&str>) -> Result<Option<String>, Error> {
        let Some(session) = self.stored()? else {
            return Ok(None);
        };
        if username.is_some_and(|username| username != session.username) {
            return Ok(None);
        }
        self.jar.store(
            &request_url(&self.portal),
            &format!("{COOKIE}={}; Path=/", session.cookie),
            unix_time().as_secs(),
        );
        if self.dashboard()? {
            return Ok(Some(session.username));
        }
        self.logout()?;
        Ok(None)
    }

    pub fn login(
        &mut self,
        username: &str,
        password: &str,
        answer: impl FnMut(Challenge) -> Result<String, Error>,
    ) -> Result<(), Error> {
        self.jar.clear();
        let url = self
            .portal
            .join("/services/simaster/service_login")
            .unwrap();
        let credentials = [("aId", ""), ("username", username), ("password", password)];
        let direct = self.send(self.http.post(url).form(&credentials))?;
        let accepted = if direct.status() == StatusCode::OK {
            let body = direct.bytes().map_err(|_| Error::Network)?;
            serde_json::from_slice::<DirectLogin>(&body).is_ok_and(|result| {
                matches!(
                    result.accepted,
                    LoginResult::Boolean(true) | LoginResult::Number(1)
                )
            })
        } else {
            false
        };
        if !accepted {
            self.cas_login(username, password, answer)?;
        }
        if !self.dashboard()? {
            return Err(Error::Failed("SIMASTER did not establish a web session"));
        }
        let session = Session {
            username: username.to_owned(),
            cookie: self
                .cookie()
                .ok_or(Error::Upstream("missing SIMASTER session cookie"))?,
        };
        let bytes = serde_json::to_vec(&session).map_err(|_| Error::Storage)?;
        self.store.set_secret(&bytes).map_err(|_| Error::Storage)
    }

    fn cas_login(
        &mut self,
        username: &str,
        password: &str,
        answer: impl FnMut(Challenge) -> Result<String, Error>,
    ) -> Result<(), Error> {
        let form = self.get(self.portal.join(SIGNIN).unwrap())?;
        if form.url().origin() != self.cas.origin() || form.url().path() != "/cas/login" {
            return Err(Error::BrowserRequired);
        }
        let url = form.url().clone();
        let html = form.text().map_err(|_| Error::Network)?;
        let ticket = hidden(&html, "lt").ok_or(Error::BrowserRequired)?;
        let response = self.send(self.http.post(url).form(&[
            ("username", username),
            ("password", password),
            ("lt", ticket.as_str()),
            ("_eventId", "submit"),
            ("submit", "LOGIN"),
        ]))?;
        let page = self.follow(response)?;
        if page.url().origin() != self.portal.origin() {
            // CAS re-renders its sign-in form when it rejects the credentials.
            return Err(match page.url().path() {
                "/cas/login" => Error::Failed(
                    "UGM CAS returned the sign-in page; check the username and password",
                ),
                _ => Error::BrowserRequired,
            });
        }
        match page.url().path() {
            CAPTCHA => self.complete_challenge(page, answer),
            _ => Ok(()),
        }
    }

    fn complete_challenge(
        &mut self,
        mut page: Response,
        mut answer: impl FnMut(Challenge) -> Result<String, Error>,
    ) -> Result<(), Error> {
        for attempt in 1..=MAX_CAPTCHA_ATTEMPTS {
            let html = page.text().map_err(|_| Error::Network)?;
            let token = hidden(&html, "simasterUGM_token").ok_or(Error::BrowserRequired)?;
            let mut asset = self.portal.join(CAPTCHA_ASSET).unwrap();
            asset.set_query(Some(&format!("_={}", unix_time().as_nanos())));
            let response = self.send(self.http.get(asset))?;
            if response.status() != StatusCode::OK {
                return Err(Error::BrowserRequired);
            }
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok());
            let extension = match content_type.and_then(|value| value.split(';').next()) {
                Some("audio/mpeg") => ".mp3",
                Some("audio/wav" | "audio/x-wav") => ".wav",
                Some("image/png") => ".png",
                Some("image/jpeg") => ".jpg",
                _ => return Err(Error::BrowserRequired),
            };
            let bytes = response.bytes().map_err(|_| Error::Network)?.to_vec();
            let answer = answer(Challenge {
                bytes,
                extension,
                rejected: attempt > 1,
            })?;
            let response =
                self.send(self.http.post(self.portal.join(CAPTCHA).unwrap()).form(&[
                    ("simasterUGM_token", token.as_str()),
                    ("captcha", answer.trim()),
                ]))?;
            let next = self.follow(response)?;
            match next.url().path() {
                // A rejected answer re-presents the challenge with a fresh token.
                CAPTCHA => page = next,
                path if path.contains("login") || path.contains("signin") => {
                    return Err(Error::Failed(
                        "SIMASTER lost the login session during CAPTCHA verification",
                    ));
                }
                _ => return Ok(()),
            }
        }
        Err(Error::Failed("SIMASTER did not accept the CAPTCHA answer"))
    }

    fn dashboard(&mut self) -> Result<bool, Error> {
        let response = self.get(self.portal.join(DASHBOARD).unwrap())?;
        Ok(response.status() == StatusCode::OK
            && response.url().path() == DASHBOARD
            && response.url().origin() == self.portal.origin()
            && self.cookie().is_some())
    }

    fn cookie(&mut self) -> Option<String> {
        let dashboard = self.portal.join(DASHBOARD).ok()?;
        let value = self
            .jar
            .cookie_header(&request_url(&dashboard), unix_time().as_secs())?;
        value.split(';').find_map(|cookie| {
            let (name, value) = cookie.trim().split_once('=')?;
            (name == COOKIE && !value.is_empty()).then(|| value.to_owned())
        })
    }

    fn get(&mut self, url: Url) -> Result<Response, Error> {
        let response = self.send(self.http.get(url))?;
        self.follow(response)
    }

    fn send(&mut self, request: RequestBuilder) -> Result<Response, Error> {
        let mut request = request.build().map_err(|_| Error::Network)?;
        let url = request.url().clone();
        let scope = request_url(&url);
        let now = unix_time().as_secs();
        if let Some(header) = self.jar.cookie_header(&scope, now) {
            request.headers_mut().insert(
                COOKIE_HEADER,
                header
                    .parse()
                    .map_err(|_| Error::Upstream("invalid session cookie"))?,
            );
        }
        let response = self.http.execute(request).map_err(|_| Error::Network)?;
        match response.status() {
            StatusCode::TOO_MANY_REQUESTS => return Err(Error::RateLimited),
            StatusCode::FORBIDDEN => return Err(Error::BrowserRequired),
            code if code.is_server_error() => {
                return Err(Error::Upstream("login service returned a server error"));
            }
            _ => {}
        }
        self.jar.store_all(
            &scope,
            response
                .headers()
                .get_all(SET_COOKIE)
                .iter()
                .map(|value| value.as_bytes()),
            now,
        );
        Ok(response)
    }

    fn follow(&mut self, mut response: Response) -> Result<Response, Error> {
        for _ in 0..8 {
            if !matches!(
                response.status(),
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
            ) {
                return Ok(response);
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(Error::Upstream("missing login redirect"))?;
            let mut url = response
                .url()
                .join(location)
                .map_err(|_| Error::Upstream("invalid login redirect"))?;
            if url.scheme() == "http"
                && self.portal.scheme() == "https"
                && url.host_str() == self.portal.host_str()
            {
                url.set_scheme("https")
                    .map_err(|_| Error::BrowserRequired)?;
            }
            if ![self.portal.origin(), self.cas.origin()].contains(&url.origin())
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(Error::BrowserRequired);
            }
            response = self.send(self.http.get(url))?;
        }
        Err(Error::BrowserRequired)
    }
}

pub mod ocr;

fn request_url(url: &Url) -> RequestUrl<'_> {
    RequestUrl::new(
        url.scheme() == "https",
        url.host_str().expect("HTTP origin"),
        url.path(),
    )
}

fn hidden(html: &str, name: &str) -> Option<String> {
    let selector = Selector::parse(&format!("input[type=hidden][name={name}]")).ok()?;
    Some(
        Html::parse_document(html)
            .select(&selector)
            .next()?
            .attr("value")?
            .to_owned(),
    )
}

#[cfg(test)]
mod tests;
