//! Browser authorization and local sessions.

pub(crate) mod oidc;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oauth2::{CsrfToken, PkceCodeVerifier};
use serde::{Deserialize, Serialize};
use url::Url;

pub const CALLBACK: &str = "http://127.0.0.1:53682/callback";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("CONFIG: set SIMASTER_CLIENT_ID to a registered UGM ID public client ID")]
    ClientId,
    #[error("AUTH_FAILED: {0}")]
    Failed(&'static str),
    #[error("NETWORK: UGM ID request failed")]
    Network,
    #[error("RATE_LIMITED: UGM ID asked the client to slow down")]
    RateLimited,
    #[error("UPSTREAM_CHANGED: {0}")]
    Upstream(&'static str),
    #[error("IO: the OS keyring could not be accessed")]
    Storage,
}

/// A one-use login attempt. Keep this in memory until the callback arrives.
pub struct PendingLogin {
    url: Url,
    state: CsrfToken,
    nonce: CsrfToken,
    verifier: PkceCodeVerifier,
    client: oidc::OidcClient,
    http: oauth2::reqwest::blocking::Client,
    jwks_url: Url,
}

impl PendingLogin {
    pub fn authorization_url(&self) -> &Url {
        &self.url
    }

    /// Validate the callback and persist the session only after verifying the ID token.
    pub fn complete(
        self,
        request_target: &str,
        store: &SessionStore,
    ) -> Result<SessionStatus, Error> {
        let code = self.callback_code(request_target)?;
        let session = oidc::exchange(self, code)?;
        store.save(&session)?;
        Ok(session.status())
    }

    fn callback_code(&self, target: &str) -> Result<String, Error> {
        let query = target
            .strip_prefix("/callback?")
            .ok_or(Error::Failed("invalid callback path"))?;
        let params = url::form_urlencoded::parse(query.as_bytes());
        let one = |key: &str| {
            let mut values = params.filter(|(name, _)| name == key);
            let (_, value) = values.next()?;
            values.next().is_none().then(|| value.into_owned())
        };
        if one("state").map(CsrfToken::new).as_ref() != Some(&self.state) {
            return Err(Error::Failed("callback state did not match"));
        }
        if params.clone().any(|(key, _)| key == "error") {
            return Err(Error::Failed("authorization was denied"));
        }
        one("code")
            .filter(|code| !code.is_empty())
            .ok_or(Error::Failed(
                "callback did not contain one authorization code",
            ))
    }
}

pub fn begin_login(client_id: &str) -> Result<PendingLogin, Error> {
    oidc::begin_login(client_id, oidc::UGM_ID)
}

/// Local credentials for the current OS user.
pub struct SessionStore(pub(crate) keyring::Entry);

impl SessionStore {
    pub fn open() -> Result<Self, Error> {
        keyring::Entry::new("simaster", "ugm-id")
            .map(Self)
            .map_err(|_| Error::Storage)
    }

    pub fn status(&self) -> Result<Option<SessionStatus>, Error> {
        match self.0.get_secret() {
            Ok(bytes) => serde_json::from_slice::<Session>(&bytes)
                .map(|session| Some(session.status()))
                .map_err(|_| Error::Storage),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::Storage),
        }
    }

    /// Remove the local session. Returns whether an entry was present.
    pub fn logout(&self) -> Result<bool, Error> {
        match self.0.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(_) => Err(Error::Storage),
        }
    }

    fn save(&self, session: &Session) -> Result<(), Error> {
        let bytes = serde_json::to_vec(session).map_err(|_| Error::Storage)?;
        self.0.set_secret(&bytes).map_err(|_| Error::Storage)
    }
}

#[derive(Serialize, Deserialize)]
struct Session {
    access_token: String,
    subject: String,
    username: Option<String>,
    scopes: Vec<String>,
    expires_at_unix: u64,
}

#[derive(Debug, Serialize)]
pub struct SessionStatus {
    pub subject: String,
    pub username: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at_unix: u64,
    pub expired: bool,
}

impl Session {
    fn status(&self) -> SessionStatus {
        SessionStatus {
            subject: self.subject.clone(),
            username: self.username.clone(),
            scopes: self.scopes.clone(),
            expires_at_unix: self.expires_at_unix,
            expired: self.expires_at_unix <= unix_time().as_secs(),
        }
    }
}

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}
