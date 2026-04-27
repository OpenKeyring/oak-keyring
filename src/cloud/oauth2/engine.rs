//! OAuth2 authorization engine — orchestrates PKCE + callback + token exchange.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::browser::open_url;
use super::callback_server::wait_for_callback;
use super::error::OAuth2Error;
use super::pkce::generate_pkce;
use super::token_store::{OAuth2Token, TokenStore};
use crate::cloud::providers::OAuth2Provider;

/// Authorization timeout (2 minutes).
const AUTH_TIMEOUT: Duration = Duration::from_secs(120);

pub struct OAuth2Engine;

impl OAuth2Engine {
    /// Execute the full OAuth2 PKCE authorization flow.
    pub async fn authorize(
        provider: &dyn OAuth2Provider,
        token_store: &TokenStore,
        cancel: CancellationToken,
    ) -> Result<OAuth2Token, OAuth2Error> {
        let (verifier, challenge) = generate_pkce();

        let auth_url = build_auth_url(provider, &challenge);
        eprintln!("请在浏览器中完成授权 (2 分钟内有效):");
        eprintln!("{}", auth_url);

        open_url(&auth_url).map_err(|msg| OAuth2Error::BrowserOpen { message: msg })?;

        let code = tokio::select! {
            code = wait_for_callback(cancel.clone()) => code?,
            _ = tokio::time::sleep(AUTH_TIMEOUT) => {
                return Err(OAuth2Error::Timeout);
            }
        };

        let token = exchange_code_for_token(provider, &code, &verifier).await?;

        token_store.save(provider.provider_id(), &token)?;

        Ok(token)
    }

    /// Revoke authorization by deleting stored tokens.
    pub fn revoke(provider_id: &str, token_store: &TokenStore) -> Result<bool, OAuth2Error> {
        token_store.delete(provider_id)
    }
}

/// Build the OAuth2 authorization URL with PKCE challenge.
fn build_auth_url(provider: &dyn OAuth2Provider, challenge: &str) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state=oak-state&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256",
        provider.auth_url(),
        urlencoding_encode(provider.client_id()),
        urlencoding_encode(provider.redirect_uri()),
        urlencoding_encode(&provider.scopes().join(" ")),
        challenge,
    )
}

/// Exchange authorization code for tokens.
async fn exchange_code_for_token(
    provider: &dyn OAuth2Provider,
    code: &str,
    verifier: &str,
) -> Result<OAuth2Token, OAuth2Error> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}&code_verifier={}",
        urlencoding_encode(code),
        urlencoding_encode(provider.redirect_uri()),
        urlencoding_encode(provider.client_id()),
        urlencoding_encode(provider.client_secret()),
        urlencoding_encode(verifier),
    );

    let mut response = ureq::post(provider.token_url())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.as_bytes())
        .map_err(|e| OAuth2Error::TokenExchange {
            message: e.to_string(),
        })?;

    let response_text =
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| OAuth2Error::TokenExchange {
                message: format!("failed to read response: {}", e),
            })?;

    let json: serde_json::Value =
        serde_json::from_str(&response_text).map_err(|e| OAuth2Error::TokenExchange {
            message: format!("failed to parse JSON response: {}", e),
        })?;

    if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
        let desc = json
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(OAuth2Error::TokenExchange {
            message: format!("{}: {}", error, desc),
        });
    }

    let access_token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OAuth2Error::TokenExchange {
            message: "missing access_token in response".to_string(),
        })?
        .to_string();

    let refresh_token = json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s: &str| s.to_string());

    let expires_in = json
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);
    let expires_at = Some(chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64));

    let token_type = json
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();

    Ok(OAuth2Token {
        access_token,
        refresh_token,
        expires_at,
        token_type,
    })
}

/// Minimal URL encoding for OAuth2 parameters.
fn urlencoding_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}
