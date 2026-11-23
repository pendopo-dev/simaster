use std::time::Duration;

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use tiny_http::{Method, Response, Server};

use crate::{
    auth::{self, PendingLogin, SessionStatus, SessionStore},
    error::Error,
    output,
};

/// Sign in and manage the local UGM ID session.
#[derive(Debug, Args)]
pub struct Auth {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Sign in through the system browser with a registered UGM ID public client.
    #[command(
        after_help = "Example: SIMASTER_CLIENT_ID=<client-id> simaster auth login\n\
        Exit codes: 1 configuration or local IO, 3 authentication, 4 network, 5 upstream response."
    )]
    Login {
        /// Use browser PKCE. Both methods require SIMASTER_CLIENT_ID.
        #[arg(long, value_enum, default_value = "auto")]
        method: LoginMethod,
    },
    /// Show safe metadata for the local session, including expiry.
    Status,
    /// Remove the local credentials from the OS keyring.
    Logout,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LoginMethod {
    Auto,
    Browser,
}

pub fn run(auth: Auth, json: bool) -> Result<(), Error> {
    match auth.command {
        AuthCommand::Login {
            method: LoginMethod::Auto | LoginMethod::Browser,
        } => {
            let client_id =
                std::env::var("SIMASTER_CLIENT_ID").map_err(|_| auth::Error::ClientId)?;
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
        AuthCommand::Status => print_status(SessionStore::open()?.status()?, "local", json),
        AuthCommand::Logout => {
            let removed = SessionStore::open()?.logout()?;
            if json {
                print_data(serde_json::json!({"removed": removed}), "local")
            } else {
                println!("Local session cleared.");
                Ok(())
            }
        }
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
    pending.complete(&target, store)
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
mod tests;
