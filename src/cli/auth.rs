use std::{
    io::{self, IsTerminal, Write},
    time::Duration,
};

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use tiny_http::{Method, Response, Server};

use crate::{
    auth::{
        self, PendingLogin, SessionStatus, SessionStore,
        web::{Challenge, Web, ocr},
    },
    error::Error,
    output,
};

/// Sign in and manage local sessions.
#[derive(Debug, Args)]
pub struct Auth {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Sign in with CLI credentials or browser PKCE.
    #[command(
        after_help = "Example: simaster auth login --method web --username <ugm-username>\n\
        Exit codes: 1 configuration or local IO, 3 authentication, 4 network, 5 upstream response."
    )]
    Login {
        /// Auto uses browser PKCE if configured, web login otherwise.
        #[arg(long, value_enum, default_value = "auto")]
        method: LoginMethod,
        /// UGM username for web login. Omit to reuse the cached account or be prompted.
        #[arg(long)]
        username: Option<String>,
    },
    /// Show safe metadata for the local session, including expiry.
    Status,
    /// Remove the local credentials from the OS keyring.
    Logout,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LoginMethod {
    Auto,
    Web,
    Browser,
}

pub fn run(auth: Auth, json: bool) -> Result<(), Error> {
    match auth.command {
        AuthCommand::Login { method, username } => {
            let client_id = std::env::var("SIMASTER_CLIENT_ID").ok();
            let use_web = matches!(method, LoginMethod::Web)
                || matches!(method, LoginMethod::Auto) && client_id.is_none();
            if use_web {
                match web_login(username.as_deref(), json) {
                    Err(Error::Auth(auth::Error::BrowserRequired)) if client_id.is_some() => {}
                    result => return result,
                }
            } else if username.is_some() {
                return Err(Error::Config("--username applies to web login".to_owned()));
            }
            let client_id = client_id.ok_or(auth::Error::ClientId)?;
            let pending = auth::begin_login(&client_id)?;
            let store = SessionStore::open()?;
            let server = Server::http(("127.0.0.1", 53682))
                .map_err(|_| Error::Other("could not listen on 127.0.0.1:53682".to_owned()))?;
            let session =
                browser_login(pending, &store, server, Duration::from_secs(300), |url| {
                    open::that_detached(url).is_ok()
                })?;
            print_status(Some(session), "ugm-id", json)
        }
        AuthCommand::Status => {
            if let Some(username) = Web::open()?.status()? {
                return print_web(&username, "local", json);
            }
            print_status(SessionStore::open()?.status()?, "local", json)
        }
        AuthCommand::Logout => {
            let web = Web::open()?.logout();
            let browser = SessionStore::open().and_then(|store| store.logout());
            let removed = web? | browser?;
            if json {
                print_data(serde_json::json!({"removed": removed}), "local")
            } else {
                println!("Local session cleared.");
                Ok(())
            }
        }
    }
}

fn web_login(username: Option<&str>, json: bool) -> Result<(), Error> {
    let mut web = Web::open()?;
    if let Some(username) = web.reuse(username)? {
        return print_web(&username, "web", json);
    }

    if !io::stdin().is_terminal() {
        return Err(Error::Config(
            "web login needs a terminal for credentials".to_owned(),
        ));
    }
    let username = match username {
        Some(username) => username.to_owned(),
        None => prompt("UGM username: ")?,
    };
    let password = rpassword::prompt_password("UGM password: ")?;
    web.login(&username, &password, assisted_answer)?;
    print_web(&username, "web", json)
}

fn prompt(label: &str) -> io::Result<String> {
    eprint!("{label}");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// Answer image challenges with the OCR model first; escalate to the human
/// once a previous answer was rejected or the model came up empty. Audio
/// challenges always reach the human.
fn assisted_answer(challenge: Challenge) -> Result<String, auth::Error> {
    if !challenge.rejected
        && let Some(answer) = ocr::classify(&challenge.bytes)
    {
        return Ok(answer);
    }
    human_answer(challenge)
}

fn human_answer(challenge: Challenge) -> Result<String, auth::Error> {
    let failure = |_| auth::Error::Failed("could not display the CAPTCHA challenge");
    let mut file = tempfile::Builder::new()
        .suffix(challenge.extension)
        .tempfile()
        .map_err(failure)?;
    file.write_all(&challenge.bytes).map_err(failure)?;
    if open::that_detached(file.path()).is_err() {
        eprintln!("Open the CAPTCHA file at {}.", file.path().display());
    }
    let answer = prompt("CAPTCHA answer: ")
        .map_err(|_| auth::Error::Failed("could not read CAPTCHA answer"))?;
    if answer.is_empty() {
        return Err(auth::Error::Failed("CAPTCHA answer is required"));
    }
    Ok(answer)
}

fn print_web(username: &str, source: &str, json: bool) -> Result<(), Error> {
    if json {
        print_data(
            serde_json::json!({
                "status": "signed_in", "provider": "web", "username": username,
                "scopes": [], "expires_at_unix": null, "storage": "keyring",
            }),
            source,
        )
    } else {
        println!("Session: signed in (web)\nAccount: {username}");
        Ok(())
    }
}

fn browser_login(
    pending: PendingLogin,
    store: &SessionStore,
    server: Server,
    timeout: Duration,
    open_browser: impl FnOnce(&str) -> bool,
) -> Result<SessionStatus, auth::Error> {
    let url = pending.authorization_url().as_str();
    if !open_browser(url) {
        eprintln!("Open this URL in your browser to continue:\n{url}");
    }
    pending.complete(&wait_for_callback(server, timeout)?, store)
}

fn wait_for_callback(server: Server, timeout: Duration) -> Result<String, auth::Error> {
    let request = server
        .recv_timeout(timeout)
        .map_err(|_| auth::Error::Failed("callback listener failed"))?
        .ok_or(auth::Error::Failed(
            "timed out waiting for the browser callback",
        ))?;
    let target = request.url().to_owned();
    let is_get = *request.method() == Method::Get;
    let _ = request.respond(Response::from_string(
        "Return to the terminal for the login result.",
    ));
    drop(server);
    if !is_get {
        return Err(auth::Error::Failed("expected a GET callback"));
    }
    Ok(target)
}

fn print_status(session: Option<SessionStatus>, source: &str, json: bool) -> Result<(), Error> {
    let status = match &session {
        Some(session) if session.expired => "expired",
        Some(_) => "signed_in",
        None => "signed_out",
    };
    if json {
        return print_data(
            serde_json::json!({
                "status": status,
                "provider": session.as_ref().map(|_| "ugm-id"),
                "username": session.as_ref().and_then(|session| session.username.as_deref()),
                "scopes": session.as_ref().map(|session| session.scopes.as_slice()).unwrap_or_default(),
                "expires_at_unix": session.as_ref().map(|session| session.expires_at_unix),
                "storage": "keyring",
            }),
            source,
        );
    }

    println!("Session: {}", status.replace('_', " "));
    if let Some(session) = session {
        let account = session.username.as_deref().unwrap_or(&session.subject);
        println!("Account: {account}");
        println!("Scopes: {}", session.scopes.join(" "));
        println!("Expires at (Unix): {}", session.expires_at_unix);
    }
    Ok(())
}

fn print_data(data: impl Serialize, source: &str) -> Result<(), Error> {
    output::print_json(&serde_json::json!({
        "schema_version": 1,
        "data": data,
        "meta": {"source": source, "fetched_at": null, "warnings": []},
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_listener_reads_one_request_and_times_out_without_one() {
        let listener = || Server::http(("127.0.0.1", 0)).unwrap();
        let server = listener();
        let address = server.server_addr().to_ip().unwrap();
        let mut client = std::net::TcpStream::connect(address).unwrap();
        client
            .write_all(b"GET /callback?code=mock-code&state=mock-state HTTP/1.0\r\n\r\n")
            .unwrap();
        assert_eq!(
            wait_for_callback(server, Duration::from_secs(2)).unwrap(),
            "/callback?code=mock-code&state=mock-state"
        );
        assert!(matches!(
            wait_for_callback(listener(), Duration::ZERO),
            Err(auth::Error::Failed(
                "timed out waiting for the browser callback"
            ))
        ));
    }
}
