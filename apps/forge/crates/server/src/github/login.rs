//! Signing in with GitHub (OAuth). Configured with `FORGE_GITHUB_CLIENT_ID`
//! and `FORGE_GITHUB_CLIENT_SECRET`, from a GitHub App (its client
//! credentials, plus the "Email addresses: read" account permission) or an
//! OAuth App, whose callback URL is `<Forge>/api/auth/github/callback`. Off
//! without them. With them, the GitHub connection also confirms who
//! installed the GitHub App (`super::setup`), which needs the App's own
//! client credentials.
//!
//! A GitHub account signs in to the Forge account it's connected to. People
//! connect it from Account while signed in (`/api/auth/github/connect`):
//! Forge doesn't verify the emails people sign up with, so matching GitHub's
//! verified email to an account with a password would let whoever signed up
//! with someone else's address take over their GitHub sign-in. Someone
//! without an account gets one (without a password) when they come from an
//! invite link; they then confirm joining on the invite page. Everyone else
//! signs up with email first, which also creates their workspace.
//!
//! The round trip to GitHub carries a signed `state` (expiry, where to go,
//! whom to connect) and a cookie with a matching nonce, so it can only be
//! finished in the browser that started it. The invite token rides in that
//! cookie, never in the URL sent to GitHub. Problems come back as codes the
//! sign-in page turns into messages, so nobody can put words on it.

use std::time::Duration;

use axum::extract::{Query, RawQuery, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use forge_domain::{clean_name, is_agent_email, normalize_email};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::{
    CurrentUser, hash_token, new_token, session_cookie, session_token, start_session,
};
use crate::invites;
use crate::state::AppState;

/// Ties a sign-in to the browser that started it, against login CSRF.
const NONCE_COOKIE: &str = "forge_github_login";
/// How long someone has to finish signing in at GitHub.
const VALID_FOR: i64 = 10 * 60;

pub struct GitHubLogin {
    client_id: String,
    client_secret: String,
    /// github.com, or a stand-in in tests.
    web: String,
    api: String,
    /// Signs the `state` round trip; new on each start of the server.
    key: [u8; 32],
    client: reqwest::Client,
}

/// What the `state` round trip remembers.
#[derive(Debug, PartialEq)]
struct Trip {
    nonce: String,
    redirect: String,
    /// Connect GitHub to this signed-in account, instead of signing in.
    connect: Option<Uuid>,
}

/// Why signing in didn't work; the sign-in page has a message for each.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Problem {
    NotSetUp,
    Expired,
    OtherBrowser,
    GitHub,
    NoEmail,
    /// An account with a password has this email: sign in with it, then
    /// connect GitHub from Account.
    UsePassword,
    NoAccount,
    InviteGone,
    /// This GitHub account is connected to another Forge account.
    TakenGitHub,
    Failed,
}

impl Problem {
    fn code(self) -> &'static str {
        match self {
            Problem::NotSetUp => "github_not_set_up",
            Problem::Expired => "github_expired",
            Problem::OtherBrowser => "github_other_browser",
            Problem::GitHub => "github_refused",
            Problem::NoEmail => "github_no_email",
            Problem::UsePassword => "github_use_password",
            Problem::NoAccount => "github_no_account",
            Problem::InviteGone => "invite_gone",
            Problem::TakenGitHub => "github_taken",
            Problem::Failed => "github_failed",
        }
    }
}

impl From<sqlx::Error> for Problem {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "GitHub sign-in failed");
        Problem::Failed
    }
}

impl GitHubLogin {
    /// From `FORGE_GITHUB_CLIENT_ID` and `FORGE_GITHUB_CLIENT_SECRET`;
    /// `None` without them.
    pub fn from_env() -> Option<Self> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Some(Self::new(
            var("FORGE_GITHUB_CLIENT_ID")?,
            var("FORGE_GITHUB_CLIENT_SECRET")?,
            "https://github.com".into(),
            "https://api.github.com".into(),
        ))
    }

    pub fn new(client_id: String, client_secret: String, web: String, api: String) -> Self {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).expect("the OS random number generator works");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("forge")
            .build()
            .expect("an HTTP client with default settings builds");
        GitHubLogin {
            client_id,
            client_secret,
            web,
            api,
            key,
            client,
        }
    }

    fn mac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC takes any key")
    }

    /// `nonce.expires.redirect.connect` (free text hex-encoded), signed.
    fn state(&self, trip: &Trip) -> String {
        let payload = format!(
            "{}.{}.{}.{}",
            trip.nonce,
            Utc::now().timestamp() + VALID_FOR,
            hex::encode(&trip.redirect),
            trip.connect
                .map(|u| u.simple().to_string())
                .unwrap_or_default(),
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
        let connect = match parts.next()? {
            "" => None,
            id => Some(id.parse().ok()?),
        };
        Some(Trip {
            nonce,
            redirect,
            connect,
        })
    }

    async fn access_token(&self, code: &str) -> Result<String, Problem> {
        let response: Value = self
            .client
            .post(format!("{}/login/oauth/access_token", self.web))
            .header("accept", "application/json")
            .json(&serde_json::json!({
                "client_id": self.client_id,
                "client_secret": self.client_secret,
                "code": code,
            }))
            .send()
            .await
            .map_err(|_| Problem::GitHub)?
            .json()
            .await
            .map_err(|_| Problem::GitHub)?;
        response["access_token"]
            .as_str()
            .map(str::to_string)
            .ok_or(Problem::GitHub)
    }

    async fn get(&self, path: &str, token: &str) -> Result<Value, Problem> {
        let response = self
            .client
            .get(format!("{}{path}", self.api))
            .bearer_auth(token)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|_| Problem::GitHub)?;
        if !response.status().is_success() {
            tracing::warn!(%path, status = %response.status(), "GitHub refused a sign-in request");
            return Err(Problem::GitHub);
        }
        response.json().await.map_err(|_| Problem::GitHub)
    }

    /// The GitHub account's id and name.
    async fn account(&self, token: &str) -> Result<(i64, String), Problem> {
        let user = self.get("/user", token).await?;
        let id = user["id"].as_i64().ok_or(Problem::GitHub)?;
        let name = user["name"]
            .as_str()
            .filter(|n| !n.trim().is_empty())
            .or(user["login"].as_str())
            .unwrap_or("GitHub user")
            .to_string();
        Ok((id, name))
    }

    /// Whether the person GitHub gave this OAuth `code` to can access the
    /// installation: GitHub lists the app's installations each person can
    /// access. `None` if GitHub didn't answer, or refused the code.
    pub async fn can_access_installation(&self, code: &str, installation_id: i64) -> Option<bool> {
        const PER_PAGE: usize = 100;
        let token = self.access_token(code).await.ok()?;
        let mut page = 1;
        loop {
            let path = format!("/user/installations?per_page={PER_PAGE}&page={page}");
            let body = self.get(&path, &token).await.ok()?;
            let installations = body["installations"].as_array()?;
            if installations
                .iter()
                .any(|i| i["id"].as_i64() == Some(installation_id))
            {
                return Some(true);
            }
            if installations.len() < PER_PAGE {
                return Some(false);
            }
            page += 1;
        }
    }

    /// The account's verified emails, primary first. Never an agent's
    /// address: people can't have those.
    async fn emails(&self, token: &str) -> Result<Vec<String>, Problem> {
        let emails = self.get("/user/emails", token).await?;
        let mut verified: Vec<(bool, String)> = emails
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["verified"].as_bool() == Some(true))
            .filter_map(|e| {
                let email = normalize_email(e["email"].as_str()?).ok()?;
                Some((e["primary"].as_bool() != Some(true), email))
            })
            .filter(|(_, email)| !is_agent_email(email))
            .collect();
        verified.sort();
        Ok(verified.into_iter().map(|(_, e)| e).collect())
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

/// Back to the sign-in screen, which explains the problem.
fn failed(problem: Problem, secure: bool) -> Response {
    let to = format!("/login?error={}", problem.code());
    redirect(&to, &[nonce_cookie("", 0, secure)])
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

/// `nonce` or `nonce.invite`, both hex, for this flow's paths only.
fn nonce_cookie(value: &str, max_age: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{NONCE_COOKIE}={value}; Path=/api/auth/github; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("a cookie made of hex and ASCII is a valid header")
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

/// Off to GitHub, remembering the trip in the state and the cookie.
fn go_to_github(
    state: &AppState,
    login: &GitHubLogin,
    redirect_to: String,
    connect: Option<Uuid>,
    invite: Option<&str>,
) -> Response {
    let nonce = new_token();
    let state_param = login.state(&Trip {
        nonce: nonce.clone(),
        redirect: redirect_to,
        connect,
    });
    // Invite tokens are hex; anything else isn't one.
    let cookie = match invite.filter(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        Some(invite) => format!("{nonce}.{invite}"),
        None => nonce,
    };
    let to = format!(
        "{}/login/oauth/authorize?client_id={}&state={}&scope=read%3Auser%20user%3Aemail",
        login.web,
        urlencode(&login.client_id),
        urlencode(&state_param),
    );
    redirect(
        &to,
        &[nonce_cookie(&cookie, VALID_FOR, state.cookie_secure)],
    )
}

#[derive(Deserialize)]
pub struct StartParams {
    invite: Option<String>,
    redirect: Option<String>,
}

/// `GET /api/auth/github`: off to GitHub to sign in.
pub async fn start(State(state): State<AppState>, Query(params): Query<StartParams>) -> Response {
    let Some(login) = state.github_login.as_ref() else {
        return failed(Problem::NotSetUp, state.cookie_secure);
    };
    let to = safe_redirect(params.redirect.as_deref());
    go_to_github(&state, login, to, None, params.invite.as_deref())
}

/// `GET /api/auth/github/connect`: off to GitHub to connect it to the
/// signed-in account, so it can sign in from then on.
pub async fn connect(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<StartParams>,
) -> Response {
    let Some(login) = state.github_login.as_ref() else {
        return failed(Problem::NotSetUp, state.cookie_secure);
    };
    let to = safe_redirect(params.redirect.as_deref());
    go_to_github(&state, login, to, Some(user.id), None)
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    /// Set when someone has just installed the GitHub App.
    installation_id: Option<String>,
}

/// `GET /api/auth/github/callback`: where GitHub sends people back.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    Query(params): Query<CallbackParams>,
) -> Response {
    // With the GitHub App's "Request user authorization (OAuth) during
    // installation" on, GitHub sends whoever installed it here instead of
    // to the setup URL, with the code that says who they are.
    if params.installation_id.is_some() {
        let query = query.unwrap_or_default();
        return redirect(&format!("/api/github/setup?{query}"), &[]);
    }
    let secure = state.cookie_secure;
    // Someone connecting GitHub is signed in already: problems go back to
    // where they started (Account), not to the sign-in page.
    let connecting = state
        .github_login
        .as_ref()
        .zip(params.state.as_deref())
        .and_then(|(login, s)| login.read_state(s))
        .filter(|trip| trip.connect.is_some())
        .map(|trip| trip.redirect);
    match finish(&state, &headers, params).await {
        Ok((Some(token), to)) => redirect(
            &to,
            &[session_cookie(&token, secure), nonce_cookie("", 0, secure)],
        ),
        Ok((None, to)) => redirect(&to, &[nonce_cookie("", 0, secure)]),
        Err(problem) => match connecting {
            Some(back) => {
                let join = if back.contains('?') { '&' } else { '?' };
                let to = format!("{back}{join}github_error={}", problem.code());
                redirect(&to, &[nonce_cookie("", 0, secure)])
            }
            None => failed(problem, secure),
        },
    }
}

/// Signs the person in (returning the new session) or connects GitHub to
/// their account; returns where to go next.
async fn finish(
    state: &AppState,
    headers: &HeaderMap,
    params: CallbackParams,
) -> Result<(Option<String>, String), Problem> {
    let login = state.github_login.as_ref().ok_or(Problem::NotSetUp)?;
    if params.error.is_some() {
        return Err(Problem::GitHub);
    }
    let trip = params
        .state
        .as_deref()
        .and_then(|s| login.read_state(s))
        .ok_or(Problem::Expired)?;
    let cookie = cookie_value(headers, NONCE_COOKIE).unwrap_or_default();
    let (nonce, invite) = match cookie.split_once('.') {
        Some((nonce, invite)) => (nonce.to_string(), Some(invite.to_string())),
        None => (cookie, None),
    };
    if nonce.is_empty() || nonce != trip.nonce {
        return Err(Problem::OtherBrowser);
    }
    let code = params.code.ok_or(Problem::GitHub)?;
    let token = login.access_token(&code).await?;
    let (github_id, name) = login.account(&token).await?;

    let mut db = state.pool.begin().await?;
    if let Some(user) = trip.connect {
        connect_account(&mut db, headers, user, github_id).await?;
        db.commit().await?;
        return Ok((None, trip.redirect));
    }

    let mut user: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE github_id = $1 AND kind = 'person'")
            .bind(github_id)
            .fetch_optional(&mut *db)
            .await?;
    if user.is_none() {
        let emails = login.emails(&token).await?;
        let existing: Option<(Uuid, bool)> = sqlx::query_as(
            "SELECT id, password_hash IS NOT NULL FROM users WHERE email = ANY($1) \
             ORDER BY array_position($1, email) LIMIT 1",
        )
        .bind(&emails)
        .fetch_optional(&mut *db)
        .await?;
        user = match existing {
            Some((_, true)) => return Err(Problem::UsePassword),
            // Accounts without a password have no other way in: theirs to claim.
            Some((id, false)) => {
                let claimed = sqlx::query(
                    "UPDATE users SET github_id = $1 WHERE id = $2 AND github_id IS NULL AND kind = 'person'",
                )
                .bind(github_id)
                .bind(id)
                .execute(&mut *db)
                .await?;
                if claimed.rows_affected() == 0 {
                    return Err(Problem::UsePassword);
                }
                Some(id)
            }
            None => {
                let invite = invite.as_deref().ok_or(Problem::NoAccount)?;
                if invites::find_valid(&mut *db, invite)
                    .await
                    .map_err(|_| Problem::Failed)?
                    .is_none()
                {
                    return Err(Problem::InviteGone);
                }
                let email = emails.first().ok_or(Problem::NoEmail)?;
                let name = clean_name(&name, "name").unwrap_or_else(|_| "GitHub user".into());
                let id = Uuid::now_v7();
                sqlx::query(
                    "INSERT INTO users (id, email, name, github_id) VALUES ($1, $2, $3, $4)",
                )
                .bind(id)
                .bind(email)
                .bind(&name)
                .bind(github_id)
                .execute(&mut *db)
                .await?;
                Some(id)
            }
        };
    }
    let user = user.ok_or(Problem::NoAccount)?;
    let session = start_session(&mut *db, user)
        .await
        .map_err(|_| Problem::Failed)?;
    db.commit().await?;
    // People confirm joining a workspace themselves, on the invite page.
    let to = match invite {
        Some(invite) => format!("/join/{invite}"),
        None => trip.redirect,
    };
    Ok((Some(session), to))
}

/// Connects the GitHub account to the person signed in in this browser,
/// who must be the one who asked.
async fn connect_account(
    db: &mut PgConnection,
    headers: &HeaderMap,
    user: Uuid,
    github_id: i64,
) -> Result<(), Problem> {
    let token = session_token(headers).ok_or(Problem::OtherBrowser)?;
    let signed_in: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM sessions WHERE token_hash = $1 AND expires_at > now()",
    )
    .bind(hash_token(&token))
    .fetch_optional(&mut *db)
    .await?;
    if signed_in != Some(user) {
        return Err(Problem::OtherBrowser);
    }
    let taken: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE github_id = $1")
        .bind(github_id)
        .fetch_optional(&mut *db)
        .await?;
    if taken.is_some_and(|other| other != user) {
        return Err(Problem::TakenGitHub);
    }
    sqlx::query("UPDATE users SET github_id = $1 WHERE id = $2")
        .bind(github_id)
        .bind(user)
        .execute(&mut *db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_and_reject_tampering() {
        let login = GitHubLogin::new("id".into(), "secret".into(), String::new(), String::new());
        let trip = Trip {
            nonce: "abc".into(),
            redirect: "/forge/issues".into(),
            connect: Some(Uuid::now_v7()),
        };
        let state = login.state(&trip);
        assert_eq!(login.read_state(&state), Some(trip));
        let tampered = state.replacen("abc", "abd", 1);
        assert_eq!(login.read_state(&tampered), None);
        let other = GitHubLogin::new("id".into(), "secret".into(), String::new(), String::new());
        assert_eq!(other.read_state(&state), None, "another server's key");
    }

    #[test]
    fn redirects_stay_on_this_site() {
        assert_eq!(safe_redirect(Some("/forge")), "/forge");
        assert_eq!(
            safe_redirect(Some("/forge/issue/FOR-1?x=1")),
            "/forge/issue/FOR-1?x=1"
        );
        for bad in [
            "//evil.example",
            "/\\evil.example",
            "/\t/evil.example",
            "/\n/evil.example",
            "/ /evil.example",
            "https://evil.example",
            "",
        ] {
            assert_eq!(safe_redirect(Some(bad)), "/", "{bad:?}");
        }
        assert_eq!(safe_redirect(None), "/");
    }
}
