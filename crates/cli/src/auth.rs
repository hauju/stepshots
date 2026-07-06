//! CLI authentication: browser-based OAuth 2.0 Authorization Code + PKCE login
//! and local token storage.
//!
//! `stepshots login` opens the dashboard's `/oauth/authorize` in the browser,
//! runs a one-shot loopback server on `127.0.0.1:<random-port>` to catch the
//! redirect, exchanges the code (with the PKCE verifier) at `/oauth/token`, and
//! stores the returned `oat_` API key at `~/.config/stepshots/tokens.json`.
//! `upload` then reads that token when `--token` / `STEPSHOTS_TOKEN` are unset.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::error::CliError;

/// Fixed public-client id, matching the dashboard's `CLI_CLIENT_ID`.
const CLIENT_ID: &str = "stepshots-cli";

/// How long to wait for the browser redirect before giving up.
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(120);

/// How long to wait for a single accepted connection to send its request.
/// Browsers open speculative connections that never send data; without this,
/// reading one of those would block the loop past `CALLBACK_TIMEOUT`.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Persisted CLI credentials (`~/.config/stepshots/tokens.json`).
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenStore {
    pub access_token: String,
    pub server: String,
}

/// Run the interactive browser login against `server`, storing the token.
pub async fn login(server: &str) -> Result<(), CliError> {
    let server = server.trim_end_matches('/');

    // PKCE + CSRF state.
    let verifier = random_url_safe(32); // 43 chars — within RFC 7636's 43–128.
    let challenge = s256_challenge(&verifier);
    let state = random_url_safe(16);

    // One-shot loopback server on a random port.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| CliError::Auth(format!("could not bind loopback server: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CliError::Auth(format!("could not read local address: {e}")))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let authorize_url = format!(
        "{server}/oauth/authorize?response_type=code&client_id={client}\
         &redirect_uri={redirect}&code_challenge={challenge}\
         &code_challenge_method=S256&state={state}",
        client = CLIENT_ID,
        redirect = percent_encode(&redirect_uri),
    );

    println!("Opening your browser to authorize Stepshots CLI…");
    if open::that(&authorize_url).is_err() {
        println!("Could not open a browser automatically.");
    }
    println!("\nIf nothing opened, visit this URL:\n{authorize_url}\n");

    // Wait for the redirect and read the authorization code.
    let code = wait_for_callback(listener, &state).await?;

    // Exchange the code (+ verifier) for the account's API key.
    let access_token = exchange_code(server, &code, &redirect_uri, &verifier).await?;

    save_tokens(&TokenStore {
        access_token,
        server: server.to_string(),
    })?;

    println!("✓ Logged in. You can now run `stepshots upload …`.");
    Ok(())
}

/// Remove stored credentials.
pub fn logout() -> Result<(), CliError> {
    match token_path() {
        Some(path) if path.exists() => {
            std::fs::remove_file(&path)
                .map_err(|e| CliError::Auth(format!("could not remove token file: {e}")))?;
            println!("✓ Logged out.");
        }
        _ => println!("Not logged in."),
    }
    Ok(())
}

/// The stored token for `server`, if any (used as an upload fallback).
pub fn stored_token_for(server: &str) -> Option<String> {
    let server = server.trim_end_matches('/');
    load_tokens()
        .filter(|t| t.server.trim_end_matches('/') == server)
        .map(|t| t.access_token)
}

/// Show which account the stored (or provided) token authenticates as by
/// validating it against `server`'s `/api/whoami` endpoint.
pub async fn whoami(server: &str, token_override: Option<String>) -> Result<(), CliError> {
    let server = server.trim_end_matches('/');

    let (token, from_store) = match token_override {
        Some(t) => (t, false),
        None => match stored_token_for(server) {
            Some(t) => (t, true),
            None => {
                return Err(CliError::Auth(format!(
                    "Not logged in to {server}. Run `stepshots login`, or set STEPSHOTS_TOKEN."
                )));
            }
        },
    };

    let resp = reqwest::Client::new()
        .get(format!("{server}/api/whoami"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| CliError::Auth(format!("could not reach {server}: {e}")))?;

    if resp.status().as_u16() == 401 {
        return Err(CliError::Auth(format!(
            "Token for {server} is invalid or expired. Run `stepshots login` again."
        )));
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or(body);
        return Err(CliError::Auth(format!(
            "whoami failed ({status}): {message}"
        )));
    }

    let info: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Auth(format!("invalid whoami response: {e}")))?;
    let email = info
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown account)");
    let name = info.get("name").and_then(|v| v.as_str()).unwrap_or("");

    println!("✓ Logged in to {server}");
    if name.is_empty() {
        println!("  {email}");
    } else {
        println!("  {email} ({name})");
    }
    if from_store {
        let path = token_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.config/stepshots/tokens.json".into());
        println!("  token: {}  ({path})", mask_token(&token));
    } else {
        println!(
            "  token: {}  (from --token / STEPSHOTS_TOKEN)",
            mask_token(&token)
        );
    }

    Ok(())
}

/// Mask a token for display, keeping only a short head and tail.
fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 8 {
        return "…".to_string();
    }
    let head: String = chars.iter().take(6).collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{head}…{tail}")
}

// ── OAuth callback handling ──────────────────────────────────────────────

async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<String, CliError> {
    let deadline = tokio::time::Instant::now() + CALLBACK_TIMEOUT;
    loop {
        let (mut stream, _) = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| CliError::Auth("timed out waiting for browser authorization".into()))?
            .map_err(|e| CliError::Auth(format!("connection error: {e}")))?;

        let mut buf = vec![0u8; 8192];
        let read_deadline = std::cmp::min(deadline, tokio::time::Instant::now() + READ_TIMEOUT);
        let n = match tokio::time::timeout_at(read_deadline, stream.read(&mut buf)).await {
            Ok(result) => result.unwrap_or(0),
            // Connection never sent data (browser preconnect) — drop it and
            // keep waiting for the real callback.
            Err(_) => 0,
        };
        if n == 0 {
            continue;
        }
        let request = String::from_utf8_lossy(&buf[..n]);
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");
        let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
        let params: HashMap<&str, &str> = query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .collect();

        if let Some(err) = params.get("error") {
            respond(
                &mut stream,
                "Authorization was cancelled. You can close this tab.",
            )
            .await;
            return Err(CliError::Auth(format!("authorization denied: {err}")));
        }

        let Some(code) = params.get("code") else {
            // Browser preconnect / favicon / bare hit — ignore and keep waiting.
            respond(&mut stream, "Waiting for authorization…").await;
            continue;
        };

        if params.get("state").copied().unwrap_or_default() != expected_state {
            respond(&mut stream, "State mismatch — please try logging in again.").await;
            return Err(CliError::Auth(
                "OAuth state mismatch (possible CSRF)".into(),
            ));
        }

        respond(
            &mut stream,
            "✓ Login successful. You can close this tab and return to your terminal.",
        )
        .await;
        return Ok((*code).to_string());
    }
}

async fn respond(stream: &mut TcpStream, message: &str) {
    let body = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Stepshots CLI</title><style>\
         body{{font-family:system-ui,-apple-system,sans-serif;background:#1c2530;color:#E1D4BB;\
         display:flex;min-height:100vh;align-items:center;justify-content:center;margin:0}}\
         .card{{text-align:center;padding:2.5rem 3rem;background:#243040;border:1px solid #537188;\
         border-radius:16px}}h2{{color:#fff;font-weight:600}}</style></head>\
         <body><div class=\"card\"><h2>{message}</h2></div></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

async fn exchange_code(
    server: &str,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<String, CliError> {
    let resp = reqwest::Client::new()
        .post(format!("{server}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|e| CliError::Auth(format!("token request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(CliError::Auth(format!(
            "token exchange failed ({status}): {body}"
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Auth(format!("invalid token response: {e}")))?;
    json.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| CliError::Auth("token response missing access_token".into()))
}

// ── PKCE helpers ─────────────────────────────────────────────────────────

fn random_url_safe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn s256_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ── Token storage ────────────────────────────────────────────────────────

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))
        .map(|base| base.join("stepshots"))
}

fn token_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("tokens.json"))
}

fn save_tokens(tokens: &TokenStore) -> Result<(), CliError> {
    let path = token_path()
        .ok_or_else(|| CliError::Auth("could not determine config directory".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::Auth(format!("could not create config directory: {e}")))?;
    }
    let json = serde_json::to_string_pretty(tokens)
        .map_err(|e| CliError::Auth(format!("could not serialize tokens: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| CliError::Auth(format!("could not write token file: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn load_tokens() -> Option<TokenStore> {
    let data = std::fs::read_to_string(token_path()?).ok()?;
    serde_json::from_str(&data).ok()
}
