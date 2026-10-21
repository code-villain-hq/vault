//! Signing in with Code Villain: one account for every studio app, kept by
//! the studio's OpenID Connect provider (Pocket ID at id.codevillain.eu).
//! Configured with `RAVEN_OIDC_ISSUER`, `RAVEN_OIDC_CLIENT_ID`,
//! `RAVEN_OIDC_CLIENT_SECRET` and `RAVEN_PUBLIC_URL` (Raven's own address;
//! the provider sends people back to `<it>/api/auth/oidc/callback`). Off
//! without them. When it's on, it's the only way people sign in: email and
//! password are closed.
//!
//! The provider says who someone is, and its group for Raven says who may
//! come in: Raven has no workspaces or invites, so a studio account the
//! provider lets through gets a Raven account the first time. It links to
//! the account with the provider's verified email if there is one.
//!
//! The round trip carries a signed `state` and a cookie with the nonce and
//! the PKCE verifier. A copy of Glyph's `oidc.rs` without invites and the
//! desktop app; keep the two alike. Raven trades the code for tokens itself,
//! with its client secret over TLS, so it checks the ID token's issuer,
//! audience, expiry and nonce but not its signature (OpenID Connect Core
//! 1.0, 3.1.3.7, step 6). The provider must therefore be https, except on
//! this machine.

use std::time::Duration;

use anyhow::bail;
use axum::extract::{Query, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::accounts::{clean_name, normalize_email};
use crate::auth::{new_token, session_cookie, start_session};
use crate::state::AppState;

/// Ties a sign-in to the browser that started it, against login CSRF.
const NONCE_COOKIE: &str = "raven_oidc_login";
/// How long someone has to finish signing in at the provider.
const VALID_FOR: i64 = 10 * 60;

pub struct OidcLogin {
    /// Without a trailing slash.
    issuer: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    /// Signs the `state` round trip; new on each start of the server.
    key: [u8; 32],
    client: reqwest::Client,
    /// The provider's endpoints, from its discovery document on first use.
    endpoints: OnceCell<Endpoints>,
}

struct Endpoints {
    authorization: String,
    token: String,
}

/// What the `state` round trip remembers.
#[derive(Debug, PartialEq)]
struct Trip {
    nonce: String,
    redirect: String,
}

/// The ID token's claims that matter here.
#[derive(Deserialize)]
struct Claims {
    iss: String,
    /// One audience or a list.
    aud: Value,
    azp: Option<String>,
    exp: i64,
    nonce: Option<String>,
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    preferred_username: Option<String>,
}

/// Why signing in didn't work; the sign-in page has a message for each.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Problem {
    NotSetUp,
    Expired,
    OtherBrowser,
    Provider,
    NoEmail,
    /// The Raven account with this email is linked to another studio account.
    Taken,
    Failed,
}

impl Problem {
    fn code(self) -> &'static str {
        match self {
            Problem::NotSetUp => "oidc_not_set_up",
            Problem::Expired => "oidc_expired",
            Problem::OtherBrowser => "oidc_other_browser",
            Problem::Provider => "oidc_refused",
            Problem::NoEmail => "oidc_no_email",
            Problem::Taken => "oidc_taken",
            Problem::Failed => "oidc_failed",
        }
    }
}

impl From<sqlx::Error> for Problem {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "Code Villain sign-in failed");
        Problem::Failed
    }
}

/// https, or plain http to this machine (a provider run locally, or tests).
fn trusted_url(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    let host = match authority.split_once(']') {
        Some((v6, _)) => v6.strip_prefix('[').unwrap_or(v6),
        None => authority.split(':').next().unwrap_or_default(),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

impl OidcLogin {
    /// From `RAVEN_OIDC_*` and `RAVEN_PUBLIC_URL`; `None` without an issuer.
    /// An issuer without the rest is a mistake, not a reason to leave
    /// passwords open.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(issuer) = var("RAVEN_OIDC_ISSUER") else {
            return Ok(None);
        };
        let (Some(client_id), Some(client_secret), Some(public_url)) = (
            var("RAVEN_OIDC_CLIENT_ID"),
            var("RAVEN_OIDC_CLIENT_SECRET"),
            var("RAVEN_PUBLIC_URL"),
        ) else {
            bail!(
                "RAVEN_OIDC_ISSUER is set: also set RAVEN_OIDC_CLIENT_ID, RAVEN_OIDC_CLIENT_SECRET and RAVEN_PUBLIC_URL"
            );
        };
        Self::new(issuer, client_id, client_secret, &public_url).map(Some)
    }

    /// `public_url` is Raven's own address, as people's browsers reach it.
    pub fn new(
        issuer: String,
        client_id: String,
        client_secret: String,
        public_url: &str,
    ) -> anyhow::Result<Self> {
        let issuer = issuer.trim_end_matches('/').to_string();
        if !trusted_url(&issuer) {
            bail!("RAVEN_OIDC_ISSUER must be an https address (or http on this machine)");
        }
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).expect("the OS random number generator works");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("raven")
            .build()
            .expect("an HTTP client with default settings builds");
        Ok(OidcLogin {
            issuer,
            client_id,
            client_secret,
            redirect_uri: format!(
                "{}/api/auth/oidc/callback",
                public_url.trim_end_matches('/')
            ),
            key,
            client,
            endpoints: OnceCell::new(),
        })
    }

    fn mac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC takes any key")
    }

    /// `nonce.expires.redirect` (the redirect hex-encoded), signed.
    fn state(&self, trip: &Trip) -> String {
        let payload = format!(
            "{}.{}.{}",
            trip.nonce,
            Utc::now().timestamp() + VALID_FOR,
            hex::encode(&trip.redirect),
        );
        let mut mac = self.mac();
        mac.update(payload.as_bytes());
        format!("{payload}.{}", hex::encode(mac.finalize().into_bytes()))
    }

    /// The trip behind a state this server made recently.
    fn read_state(&self, state: &str) -> Option<Trip> {
        let (payload, signature) = state.rsplit_once('.')?;
        let mut mac = self.mac();
        mac.update(payload.as_bytes());
        mac.verify_slice(&hex::decode(signature).ok()?).ok()?;
        let mut parts = payload.split('.');
        let nonce = parts.next()?.to_string();
        let expires: i64 = parts.next()?.parse().ok()?;
        if expires < Utc::now().timestamp() {
            return None;
        }
        let redirect = String::from_utf8(hex::decode(parts.next()?).ok()?).ok()?;
        Some(Trip { nonce, redirect })
    }

    async fn endpoints(&self) -> Result<&Endpoints, Problem> {
        self.endpoints
            .get_or_try_init(|| async {
                let url = format!("{}/.well-known/openid-configuration", self.issuer);
                let response = self.client.get(&url).send().await.map_err(|e| {
                    tracing::warn!(error = %e, "the sign-in provider didn't answer");
                    Problem::Provider
                })?;
                if !response.status().is_success() {
                    tracing::warn!(status = %response.status(), "the sign-in provider refused discovery");
                    return Err(Problem::Provider);
                }
                let doc: Value = response.json().await.map_err(|_| Problem::Provider)?;
                let issuer = doc["issuer"].as_str().map(|i| i.trim_end_matches('/'));
                let authorization = doc["authorization_endpoint"].as_str();
                let token = doc["token_endpoint"].as_str().filter(|t| trusted_url(t));
                match (issuer, authorization, token) {
                    (Some(issuer), Some(authorization), Some(token)) if issuer == self.issuer => {
                        Ok(Endpoints {
                            authorization: authorization.to_string(),
                            token: token.to_string(),
                        })
                    }
                    _ => {
                        tracing::warn!("the sign-in provider's discovery document doesn't fit RAVEN_OIDC_ISSUER");
                        Err(Problem::Provider)
                    }
                }
            })
            .await
    }

    /// Trades the code for the ID token's claims.
    async fn claims(&self, code: &str, verifier: &str) -> Result<Claims, Problem> {
        let endpoints = self.endpoints().await?;
        let response = self
            .client
            .post(&endpoints.token)
            .basic_auth(
                urlencode(&self.client_id),
                Some(urlencode(&self.client_secret)),
            )
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &self.redirect_uri),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .map_err(|_| Problem::Provider)?;
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), "the sign-in provider refused the code");
            return Err(Problem::Provider);
        }
        let body: Value = response.json().await.map_err(|_| Problem::Provider)?;
        let id_token = body["id_token"].as_str().ok_or(Problem::Provider)?;
        jsonwebtoken::dangerous::insecure_decode::<Claims>(id_token)
            .map(|data| data.claims)
            .map_err(|e| {
                tracing::warn!(error = %e, "the sign-in provider's ID token didn't read");
                Problem::Provider
            })
    }

    /// The token is for Raven, from this sign-in, and still current.
    fn check(&self, claims: &Claims, nonce: &str) -> Result<(), Problem> {
        let for_us = match &claims.aud {
            Value::String(aud) => *aud == self.client_id,
            Value::Array(aud) => aud.iter().any(|a| a.as_str() == Some(&self.client_id)),
            _ => false,
        };
        let fits = claims.iss.trim_end_matches('/') == self.issuer
            && for_us
            && claims.azp.as_ref().is_none_or(|azp| *azp == self.client_id)
            && claims.exp > Utc::now().timestamp()
            && claims.nonce.as_deref() == Some(nonce)
            && !claims.sub.is_empty();
        if !fits {
            tracing::warn!("the sign-in provider's ID token doesn't fit this sign-in");
            return Err(Problem::Provider);
        }
        Ok(())
    }
}

/// A path on this site, nothing that a browser could read as another one:
/// no control characters, spaces or backslashes, and not `//`.
fn safe_redirect(path: Option<&str>) -> String {
    path.filter(|p| {
        p.starts_with('/')
            && !p.starts_with("//")
            && p.chars().all(|c| c.is_ascii_graphic() && c != '\\')
    })
    .unwrap_or("/")
    .to_string()
}

fn redirect(to: &str, cookies: &[HeaderValue]) -> Response {
    let mut response = (StatusCode::SEE_OTHER, [(LOCATION, to.to_string())]).into_response();
    for cookie in cookies {
        response.headers_mut().append(SET_COOKIE, cookie.clone());
    }
    response
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then(|| value.to_string())
        })
}

/// `nonce.verifier`, both hex, for this flow's paths only.
fn nonce_cookie(value: &str, max_age: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{NONCE_COOKIE}={value}; Path=/api/auth/oidc; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("a cookie made of hex and ASCII is a valid header")
}

/// Back to the sign-in screen, which explains the problem.
fn failed(problem: Problem, secure: bool) -> Response {
    let to = format!("/login?error={}", problem.code());
    redirect(&to, &[nonce_cookie("", 0, secure)])
}

#[derive(Deserialize)]
pub struct StartParams {
    redirect: Option<String>,
}

/// `GET /api/auth/oidc`: off to the provider to sign in.
pub async fn start(State(state): State<AppState>, Query(params): Query<StartParams>) -> Response {
    let secure = state.cookie_secure;
    let Some(login) = state.oidc_login.as_ref() else {
        return failed(Problem::NotSetUp, secure);
    };
    let endpoints = match login.endpoints().await {
        Ok(endpoints) => endpoints,
        Err(problem) => return failed(problem, secure),
    };
    let nonce = new_token();
    let verifier = new_token();
    let state_param = login.state(&Trip {
        nonce: nonce.clone(),
        redirect: safe_redirect(params.redirect.as_deref()),
    });
    let cookie = format!("{nonce}.{verifier}");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let join = if endpoints.authorization.contains('?') {
        '&'
    } else {
        '?'
    };
    let to = format!(
        "{}{join}response_type=code&client_id={}&redirect_uri={}&scope=openid%20email%20profile\
         &state={}&nonce={nonce}&code_challenge={challenge}&code_challenge_method=S256",
        endpoints.authorization,
        urlencode(&login.client_id),
        urlencode(&login.redirect_uri),
        urlencode(&state_param),
    );
    redirect(&to, &[nonce_cookie(&cookie, VALID_FOR, secure)])
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// `GET /api/auth/oidc/callback`: where the provider sends people back.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Response {
    let secure = state.cookie_secure;
    match finish(&state, &headers, params).await {
        Ok((session, to)) => redirect(
            &to,
            &[
                session_cookie(&session, secure),
                nonce_cookie("", 0, secure),
            ],
        ),
        Err(problem) => failed(problem, secure),
    }
}

/// Signs in, returning the new session and where to go.
async fn finish(
    state: &AppState,
    headers: &HeaderMap,
    params: CallbackParams,
) -> Result<(String, String), Problem> {
    let login = state.oidc_login.as_ref().ok_or(Problem::NotSetUp)?;
    if params.error.is_some() {
        return Err(Problem::Provider);
    }
    let trip = params
        .state
        .as_deref()
        .and_then(|s| login.read_state(s))
        .ok_or(Problem::Expired)?;
    let cookie = cookie_value(headers, NONCE_COOKIE).unwrap_or_default();
    let (nonce, verifier) = cookie.split_once('.').unwrap_or_default();
    if nonce.is_empty() || nonce != trip.nonce {
        return Err(Problem::OtherBrowser);
    }
    let code = params.code.ok_or(Problem::Provider)?;
    let claims = login.claims(&code, verifier).await?;
    login.check(&claims, &trip.nonce)?;

    let mut db = state.pool.begin().await?;
    let user = account(&mut db, &claims).await?;
    let session = start_session(&mut *db, user)
        .await
        .map_err(|_| Problem::Failed)?;
    db.commit().await?;
    Ok((session, trip.redirect))
}

/// The Raven account behind the studio account: linked already, linked now
/// by email, or new.
async fn account(db: &mut PgConnection, claims: &Claims) -> Result<Uuid, Problem> {
    let linked: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE oidc_sub = $1")
        .bind(&claims.sub)
        .fetch_optional(&mut *db)
        .await?;
    if let Some(user) = linked {
        return Ok(user);
    }
    // An email the provider says it hasn't verified proves nothing.
    let email = claims
        .email
        .as_deref()
        .filter(|_| claims.email_verified != Some(false))
        .and_then(|e| normalize_email(e).ok())
        .ok_or(Problem::NoEmail)?;
    let existing: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT id, oidc_sub IS NULL FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&mut *db)
            .await?;
    match existing {
        Some((user, true)) => {
            sqlx::query("UPDATE users SET oidc_sub = $1 WHERE id = $2")
                .bind(&claims.sub)
                .bind(user)
                .execute(&mut *db)
                .await?;
            Ok(user)
        }
        Some((_, false)) => Err(Problem::Taken),
        None => {
            let fallback = email.split('@').next().unwrap_or_default().to_string();
            let name = [&claims.name, &claims.preferred_username]
                .into_iter()
                .flatten()
                .chain([&fallback])
                .find_map(|n| clean_name(n, "name").ok())
                .unwrap_or_else(|| "New member".into());
            let user = Uuid::now_v7();
            sqlx::query("INSERT INTO users (id, email, name, oidc_sub) VALUES ($1, $2, $3, $4)")
                .bind(user)
                .bind(&email)
                .bind(&name)
                .bind(&claims.sub)
                .execute(&mut *db)
                .await?;
            Ok(user)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login() -> OidcLogin {
        OidcLogin::new(
            "https://id.example".into(),
            "raven".into(),
            "secret".into(),
            "https://glyph.example/",
        )
        .unwrap()
    }

    #[test]
    fn states_round_trip_and_reject_tampering() {
        let ours = login();
        let trip = Trip {
            nonce: "abc".into(),
            redirect: "/forge/technical".into(),
        };
        let state = ours.state(&trip);
        assert_eq!(ours.read_state(&state), Some(trip));
        assert_eq!(ours.read_state(&state.replacen("abc", "abd", 1)), None);
        assert_eq!(login().read_state(&state), None, "another server's key");
    }

    #[test]
    fn providers_are_https_except_on_this_machine() {
        for good in [
            "https://id.codevillain.eu",
            "http://localhost:7411",
            "http://127.0.0.1",
            "http://[::1]:7411/x",
            "http://[::1]",
        ] {
            assert!(trusted_url(good), "{good}");
        }
        for bad in [
            "http://id.codevillain.eu",
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example/",
            "ftp://localhost",
        ] {
            assert!(!trusted_url(bad), "{bad}");
        }
    }
}
