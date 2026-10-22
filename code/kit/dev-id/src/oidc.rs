//! Signing in, as Pocket ID does it: discovery, an authorize page that asks
//! who you are, and the token endpoint. `login_hint` (an id or email) skips
//! the page, for scripts.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{APPS, DevId, SECRET};

/// The key the ID tokens are signed with. Apps don't check the signature of
/// a token they fetched themselves (OpenID Connect Core 3.1.3.7).
const KEY: &[u8] = b"dev-id";

pub fn router(dev: Arc<DevId>) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/jwks", get(|| async { axum::Json(json!({ "keys": [] })) }))
        .with_state(dev)
}

async fn discovery(State(dev): State<Arc<DevId>>) -> impl IntoResponse {
    let issuer = &dev.issuer;
    axum::Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["HS256"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": ["openid", "email", "profile", "groups"],
    }))
}

/// What the code carries to the token endpoint.
#[derive(Serialize, Deserialize)]
struct Code {
    who: String,
    client: String,
    redirect_uri: String,
    nonce: Option<String>,
    challenge: Option<String>,
}

fn app_name(client: &str) -> Option<&'static str> {
    APPS.iter()
        .find(|(id, ..)| *id == client)
        .map(|(_, name, _)| *name)
}

async fn authorize(
    State(dev): State<Arc<DevId>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let param = |name: &str| params.get(name).cloned().filter(|v| !v.is_empty());
    let (Some(client), Some(redirect_uri)) = (param("client_id"), param("redirect_uri")) else {
        return (
            StatusCode::BAD_REQUEST,
            "client_id and redirect_uri are needed",
        )
            .into_response();
    };
    let Some(app) = app_name(&client) else {
        return (
            StatusCode::BAD_REQUEST,
            format!("dev-id has no client {client}"),
        )
            .into_response();
    };
    let people = match dev.people() {
        Ok(people) => people,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    if let Some(person) = param("login_hint").and_then(|who| people.find(&who).cloned()) {
        let code = Code {
            who: person.id,
            client,
            redirect_uri: redirect_uri.clone(),
            nonce: param("nonce"),
            challenge: param("code_challenge"),
        };
        let code = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&code).expect("plain data"));
        let mut back = format!(
            "{redirect_uri}{}code={code}",
            if redirect_uri.contains('?') { '&' } else { '?' }
        );
        if let Some(state) = param("state") {
            back.push_str(&format!("&state={}", encode(&state)));
        }
        return Redirect::to(&back).into_response();
    }

    // Ask who's signing in: each person is this same address with their id
    // as the hint.
    let query: String = params
        .iter()
        .filter(|(k, _)| k.as_str() != "login_hint")
        .map(|(k, v)| format!("{}={}&", encode(k), encode(v)))
        .collect();
    let rows: String = people
        .person
        .iter()
        .map(|p| {
            let groups = if p.groups.is_empty() {
                "in no group".to_string()
            } else {
                p.groups.join(", ")
            };
            format!(
                "<a class=\"person\" href=\"/authorize?{query}login_hint={id}\"><b>{name}</b><span>{email}</span><small>{groups}</small></a>",
                id = encode(&p.id),
                name = escape(&p.name),
                email = escape(&p.email),
                groups = escape(&groups),
            )
        })
        .collect();
    Html(format!(
        r#"<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Sign in to {app}</title>
<style>
/* Nightshade, night and day (studio/design/tokens/color.css) */
:root {{ color-scheme: dark; --bg: #0B0B0B; --surface: #1B1B1B; --line: #2B2B2B; --fg: #EBEBEB; --muted: #9B9B9B; --accent: #AF6BC8; }}
@media (prefers-color-scheme: light) {{ :root {{ color-scheme: light; --bg: #F5F5F5; --surface: #F0F0F0; --line: #E1E1E1; --fg: #181818; --muted: #555555; --accent: #5A1470; }} }}
body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg); font: 14px/1.4 system-ui, sans-serif; }}
main {{ width: min(380px, calc(100vw - 32px)); display: grid; gap: 12px; }}
h1 {{ font-size: 16px; font-weight: 600; margin: 0; }}
p {{ margin: 0; color: var(--muted); }}
.person {{ display: grid; gap: 2px; padding: 10px 12px; border: 1px solid var(--line); border-radius: 8px; background: var(--surface); color: inherit; text-decoration: none; }}
.person:hover {{ border-color: var(--accent); }}
.person span, .person small {{ color: var(--muted); }}
</style>
<main><h1>Sign in to {app} with {company}</h1><p>dev-id, the stand-in account for local development. Pick who you are.</p>{rows}</main>"#,
        company = escape(&people.company),
    ))
    .into_response()
}

async fn token(
    State(dev): State<Arc<DevId>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let error = |what: &str| {
        (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": what })),
        )
            .into_response()
    };
    // The client's secret, by Basic auth or in the form.
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|v| {
            v.split_once(':')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        });
    let (client, secret) = basic.unwrap_or_else(|| {
        (
            form.get("client_id").cloned().unwrap_or_default(),
            form.get("client_secret").cloned().unwrap_or_default(),
        )
    });
    if app_name(&client).is_none() || secret != SECRET {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({ "error": "invalid_client" })),
        )
            .into_response();
    }
    if form.get("grant_type").map(String::as_str) != Some("authorization_code") {
        return error("unsupported_grant_type");
    }
    let Some(code) = form
        .get("code")
        .and_then(|c| URL_SAFE_NO_PAD.decode(c).ok())
        .and_then(|c| serde_json::from_slice::<Code>(&c).ok())
    else {
        return error("invalid_grant");
    };
    if code.client != client || form.get("redirect_uri") != Some(&code.redirect_uri) {
        return error("invalid_grant");
    }
    if let Some(challenge) = &code.challenge {
        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
        if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != *challenge {
            return error("invalid_grant");
        }
    }
    let people = match dev.people() {
        Ok(people) => people,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let Some(person) = people.find(&code.who) else {
        return error("invalid_grant");
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    let mut claims = json!({
        "iss": dev.issuer,
        "aud": client,
        "sub": person.id,
        "email": person.email,
        "email_verified": true,
        "name": person.name,
        "preferred_username": person.id,
        "groups": person.groups,
        "iat": now,
        "exp": now + 600,
    });
    if let Some(nonce) = code.nonce {
        claims["nonce"] = json!(nonce);
    }
    axum::Json(json!({
        "access_token": URL_SAFE_NO_PAD.encode(person.id.as_bytes()),
        "token_type": "Bearer",
        "expires_in": 600,
        "id_token": sign(&claims),
    }))
    .into_response()
}

fn sign(claims: &serde_json::Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    let mut mac = Hmac::<Sha256>::new_from_slice(KEY).expect("HMAC takes any key");
    mac.update(format!("{header}.{payload}").as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{header}.{payload}.{signature}")
}

/// Percent-encodes everything but unreserved characters.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
