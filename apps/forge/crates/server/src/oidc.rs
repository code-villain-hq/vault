//! Signing in with Code Villain: one account for every studio app, kept by
//! the studio's OpenID Connect provider (Pocket ID at id.codevillain.eu).
//! Configured with `FORGE_OIDC_ISSUER`, `FORGE_OIDC_CLIENT_ID`,
//! `FORGE_OIDC_CLIENT_SECRET` and `FORGE_PUBLIC_URL` (Forge's own address;
//! the provider sends people back to `<it>/api/auth/oidc/callback`). Off
//! without them. When it's on, it's the only way people sign in: email and
//! password, and GitHub, are closed.
//!
//! The provider says who someone is; Forge says what they may do here
//! (workspaces, roles and invites stay in Forge). A studio account signs in
//! to the Forge account it's linked to (`users.oidc_sub`). The first time, it
//! links to the account with the provider's verified email: the studio keeps
//! those emails, and with this on nobody can sign up here with an address of
//! their choosing. Someone without an account gets one when they come from an
//! invite link, and confirms joining on the invite page.
//!
//! The round trip carries a signed `state` and a cookie with the nonce, the
//! PKCE verifier and any invite, as signing in with GitHub does
//! (`github::login`). Forge trades the code for tokens itself, with its
//! client secret over TLS, so it checks the ID token's issuer, audience,
//! expiry and nonce but not its signature (OpenID Connect Core 1.0,
//! 3.1.3.7, step 6). The provider must therefore be https, except on this
//! machine.
//!
//! The desktop app can't see the browser's cookies. It opens the system
//! browser at `/api/auth/oidc?desktop=<challenge>`, the hex SHA-256 of a
//! secret it keeps. Once signed in, the browser lands on the web app's
//! `/signed-in?code=<code>&next=<path>` page, which opens
//! `forge://signed-in` with the same query and tells the person they can
//! close the tab. The app trades that one-time code and its secret at
//! `POST /api/auth/oidc/desktop` for a session.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::bail;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use forge_domain::{clean_name, is_agent_email, normalize_email};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::accounts::signed_in;
use crate::auth::{hash_token, new_token, session_cookie, start_session};
use crate::error::AppError;
use crate::github::login::{cookie_value, redirect, safe_redirect, urlencode};
use crate::invites;
use crate::state::AppState;

/// Ties a sign-in to the browser that started it, against login CSRF.
const NONCE_COOKIE: &str = "forge_oidc_login";
/// How long someone has to finish signing in at the provider.
const VALID_FOR: i64 = 10 * 60;
/// The web app's page that hands a desktop sign-in back to the app.
pub const DESKTOP_RETURN: &str = "/signed-in";
/// How long the desktop app has to collect its session.
const HANDOFF_FOR: Duration = Duration::from_secs(5 * 60);

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
    /// Desktop sign-ins waiting for the app, by the hash of their code.
    handoffs: Mutex<HashMap<Vec<u8>, Handoff>>,
}

struct Endpoints {
    authorization: String,
    token: String,
}

struct Handoff {
    user: Uuid,
    challenge: String,
    until: Instant,
}

/// What the `state` round trip remembers.
#[derive(Debug, PartialEq)]
struct Trip {
    nonce: String,
    redirect: String,
    /// A desktop sign-in: the challenge the app's secret must answer.
    desktop: Option<String>,
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
    NoAccount,
    InviteGone,
    /// The Forge account with this email is linked to another studio account.
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
            Problem::NoAccount => "oidc_no_account",
            Problem::InviteGone => "invite_gone",
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
    /// From `FORGE_OIDC_*` and `FORGE_PUBLIC_URL`; `None` without an issuer.
    /// An issuer without the rest is a mistake, not a reason to leave
    /// passwords open.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(issuer) = var("FORGE_OIDC_ISSUER") else {
            return Ok(None);
        };
        let (Some(client_id), Some(client_secret), Some(public_url)) = (
            var("FORGE_OIDC_CLIENT_ID"),
            var("FORGE_OIDC_CLIENT_SECRET"),
            var("FORGE_PUBLIC_URL"),
        ) else {
            bail!(
                "FORGE_OIDC_ISSUER is set: also set FORGE_OIDC_CLIENT_ID, FORGE_OIDC_CLIENT_SECRET and FORGE_PUBLIC_URL"
            );
        };
        Self::new(issuer, client_id, client_secret, &public_url).map(Some)
    }

    /// `public_url` is Forge's own address, as people's browsers reach it.
    pub fn new(
        issuer: String,
        client_id: String,
        client_secret: String,
        public_url: &str,
    ) -> anyhow::Result<Self> {
        let issuer = issuer.trim_end_matches('/').to_string();
        if !trusted_url(&issuer) {
            bail!("FORGE_OIDC_ISSUER must be an https address (or http on this machine)");
        }
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).expect("the OS random number generator works");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("forge")
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
            handoffs: Mutex::default(),
        })
    }

    fn mac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC takes any key")
    }

    /// `nonce.expires.redirect.desktop` (free text hex-encoded), signed.
    fn state(&self, trip: &Trip) -> String {
        let payload = format!(
            "{}.{}.{}.{}",
            trip.nonce,
            Utc::now().timestamp() + VALID_FOR,
            hex::encode(&trip.redirect),
            trip.desktop.as_deref().unwrap_or_default(),
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
        let desktop = Some(parts.next()?.to_string()).filter(|d| !d.is_empty());
        Some(Trip {
            nonce,
            redirect,
            desktop,
        })
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
                        tracing::warn!("the sign-in provider's discovery document doesn't fit FORGE_OIDC_ISSUER");
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

    /// The token is for Forge, from this sign-in, and still current.
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

    /// A one-time code the desktop app trades for a session.
    fn hand_off(&self, user: Uuid, challenge: String) -> String {
        let code = new_token();
        let now = Instant::now();
        let mut handoffs = self.handoffs.lock().expect("no handoff panics while held");
        handoffs.retain(|_, h| h.until > now);
        handoffs.insert(
            hash_token(&code),
            Handoff {
                user,
                challenge,
                until: now + HANDOFF_FOR,
            },
        );
        code
    }

    /// Who signed in with this code, if the secret answers its challenge.
    /// A code works once, right or wrong.
    fn take(&self, code: &str, secret: &str) -> Option<Uuid> {
        let handoff = self
            .handoffs
            .lock()
            .expect("no handoff panics while held")
            .remove(&hash_token(code))?;
        let answered = hex::encode(Sha256::digest(secret.as_bytes())) == handoff.challenge;
        (answered && handoff.until > Instant::now()).then_some(handoff.user)
    }
}

/// Hex of the given length: tokens and challenges, nothing else.
fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `nonce.verifier` or `nonce.verifier.invite`, all hex, for this flow's
/// paths only.
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
    invite: Option<String>,
    redirect: Option<String>,
    /// From the desktop app: the hex SHA-256 of its secret.
    desktop: Option<String>,
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
        desktop: params
            .desktop
            .filter(|d| is_hex(d, 64))
            .map(|d| d.to_ascii_lowercase()),
    });
    // The invite rides in the cookie, never in the URL sent to the provider.
    let cookie = match params.invite.filter(|i| is_hex(i, 64)) {
        Some(invite) => format!("{nonce}.{verifier}.{invite}"),
        None => format!("{nonce}.{verifier}"),
    };
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

/// How a sign-in ends.
enum Done {
    Browser { session: String, to: String },
    Desktop { code: String, to: String },
}

/// `GET /api/auth/oidc/callback`: where the provider sends people back.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Response {
    let secure = state.cookie_secure;
    let clear = nonce_cookie("", 0, secure);
    match finish(&state, &headers, params).await {
        Ok(Done::Browser { session, to }) => {
            redirect(&to, &[session_cookie(&session, secure), clear])
        }
        Ok(Done::Desktop { code, to }) => redirect(
            &format!("{DESKTOP_RETURN}?code={code}&next={}", urlencode(&to)),
            &[clear],
        ),
        Err(problem) => failed(problem, secure),
    }
}

async fn finish(
    state: &AppState,
    headers: &HeaderMap,
    params: CallbackParams,
) -> Result<Done, Problem> {
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
    let mut parts = cookie.split('.');
    let nonce = parts.next().unwrap_or_default();
    let verifier = parts.next().unwrap_or_default();
    let invite = parts.next();
    if nonce.is_empty() || nonce != trip.nonce {
        return Err(Problem::OtherBrowser);
    }
    let code = params.code.ok_or(Problem::Provider)?;
    let claims = login.claims(&code, verifier).await?;
    login.check(&claims, &trip.nonce)?;

    let mut db = state.pool.begin().await?;
    let user = account(&mut db, &claims, invite).await?;
    // People confirm joining a workspace themselves, on the invite page.
    let to = match invite {
        Some(invite) => format!("/join/{invite}"),
        None => trip.redirect,
    };
    match trip.desktop {
        Some(challenge) => {
            db.commit().await?;
            let code = login.hand_off(user, challenge);
            Ok(Done::Desktop { code, to })
        }
        None => {
            let session = start_session(&mut *db, user)
                .await
                .map_err(|_| Problem::Failed)?;
            db.commit().await?;
            Ok(Done::Browser { session, to })
        }
    }
}

/// The Forge account behind the studio account: linked already, linked now
/// by email, or new from an invite.
async fn account(
    db: &mut PgConnection,
    claims: &Claims,
    invite: Option<&str>,
) -> Result<Uuid, Problem> {
    let linked: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE oidc_sub = $1 AND kind = 'person'")
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
        .filter(|e| !is_agent_email(e))
        .ok_or(Problem::NoEmail)?;
    let existing: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, kind = 'person' AND oidc_sub IS NULL FROM users WHERE email = $1",
    )
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
            let invite = invite.ok_or(Problem::NoAccount)?;
            if invites::find_valid(&mut *db, invite)
                .await
                .map_err(|_| Problem::Failed)?
                .is_none()
            {
                return Err(Problem::InviteGone);
            }
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

#[derive(Deserialize)]
pub struct DesktopRequest {
    code: String,
    secret: String,
}

/// `POST /api/auth/oidc/desktop`: the desktop app collects the session its
/// browser sign-in started.
pub async fn desktop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DesktopRequest>,
) -> Result<Response, AppError> {
    let login = state
        .oidc_login
        .as_ref()
        .ok_or_else(|| AppError::not_found("signing in with Code Villain is off"))?;
    let user = login
        .take(&request.code, &request.secret)
        .ok_or_else(|| AppError::unauthorized("this sign-in has expired: start again"))?;
    let token = start_session(&state.pool, user).await?;
    signed_in(&state, &headers, &token, user).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login() -> OidcLogin {
        OidcLogin::new(
            "https://id.example".into(),
            "forge".into(),
            "secret".into(),
            "https://forge.example/",
        )
        .unwrap()
    }

    #[test]
    fn states_round_trip_and_reject_tampering() {
        let ours = login();
        let trip = Trip {
            nonce: "abc".into(),
            redirect: "/forge/issues".into(),
            desktop: Some("ab".repeat(32)),
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

    #[test]
    fn desktop_codes_work_once_and_only_with_the_secret() {
        let login = login();
        let user = Uuid::now_v7();
        let challenge = hex::encode(Sha256::digest(b"the app's secret"));
        let code = login.hand_off(user, challenge.clone());
        assert_eq!(login.take(&code, "a guess"), None);
        assert_eq!(
            login.take(&code, "the app's secret"),
            None,
            "spent by the guess"
        );

        let code = login.hand_off(user, challenge);
        assert_eq!(login.take(&code, "the app's secret"), Some(user));
        assert_eq!(login.take(&code, "the app's secret"), None);
    }
}
