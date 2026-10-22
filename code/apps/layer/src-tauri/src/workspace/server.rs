//! Workspaces on a Layer server (PROF-15 to PROF-18): signing in with the
//! server's company, and opening a workspace in a window of its own. A
//! server's people come from its company's account, so there is nothing to
//! join here. The calls are
//! made here rather than from the window, so the session goes from the
//! server straight to the keychain (PROF-16) and never through a page.
//!
//! A sign-in is held in this copy of the app (`PENDING`) while the person
//! chooses a workspace. The copy that opens it is handed the session
//! through that workspace's own keychain item.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use layer_domain::{Me, Role, WorkspaceInfo};
use parking_lot::Mutex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::Place;
use crate::config::shared::WorkspaceAt;
use crate::error::{Error, Result};
use crate::profiles::{self, Entry};

/// What the server's own headers are called (`crates/server/src/auth.rs`).
const CLIENT_HEADER: &str = "x-layer-client";
const SESSION_HEADER: &str = "x-layer-session";
/// As long as the server keeps a sign-in's code (PROF-17).
const WAIT: Duration = Duration::from_secs(5 * 60);
/// Where a copy that is not waiting leaves a sign-in for the one that is.
const RELAY: &str = "signed-in.json";

/// Why a server workspace has no session: the window offers Sign in again.
pub const SIGNED_OUT: &str = "signed out";

/// How a server signs people in: with its company's account, or not at
/// all (another Mac's own server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ways {
    pub oidc: bool,
    /// The company the server runs for, as the sign-in button names it.
    #[serde(default)]
    pub company: Option<String>,
}

/// What the window shows once signed in: the person's workspaces there.
#[derive(Debug, Clone, Serialize)]
pub struct SignedIn {
    pub host: String,
    pub company: String,
    pub email: String,
    pub workspaces: Vec<WorkspaceInfo>,
}

struct Signed {
    origin: String,
    token: String,
    me: Me,
}

static PENDING: Mutex<Option<Signed>> = Mutex::new(None);

/// What `layer://signed-in` brought: a code to trade, or the problem's
/// code (`problem_words`) when there is none.
type Back = std::result::Result<String, String>;

/// The browser sign-in this copy waits for, by turn: a newer one replaces it.
static WAITING: Mutex<Option<(u64, oneshot::Sender<Back>)>> = Mutex::new(None);
static TURN: AtomicU64 = AtomicU64::new(0);

/// A server address as the person types it, as the server's origin. Any
/// path is the server's, and dropped.
pub fn read_address(address: &str) -> Result<String> {
    let address = address.trim().trim_end_matches('/');
    let local = ["localhost", "127.0.0.1", "[::1]"].iter().any(|h| address.starts_with(h));
    let full = match address {
        a if a.contains("://") => a.to_string(),
        a if local => format!("http://{a}"),
        a => format!("https://{a}"),
    };
    let wrong = || Error::Other(format!("“{address}” is not a server address."));
    let url = Url::parse(&full).map_err(|_| wrong())?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(Error::Other("A server is reached over https.".into()));
    }
    Ok(url.origin().ascii_serialization())
}

/// The server's name as people say it: its host, and its port if it is
/// not the usual one. `read_address` reads it back.
pub fn host(origin: &str) -> String {
    let Ok(url) = Url::parse(origin) else { return origin.to_string() };
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        _ => origin.to_string(),
    }
}

fn unreachable(origin: &str) -> impl Fn(reqwest::Error) -> Error + '_ {
    move |_| Error::Other(format!("Layer couldn't reach {}. Check the address, and that you're online.", host(origin)))
}

/// What the server said it refused, in its words.
async fn refused(response: reqwest::Response) -> Error {
    let status = response.status();
    let said = response.json::<Value>().await.ok().and_then(|b| b["message"].as_str().map(str::to_string));
    Error::Other(said.map(|s| capitalised(&s)).unwrap_or_else(|| format!("The server answered {status}.")))
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    let first: String = chars.next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    let text = first + chars.as_str();
    if text.ends_with('.') { text } else { text + "." }
}

pub async fn ways(origin: &str) -> Result<Ways> {
    let response = crate::integrations::http_client()
        .get(format!("{origin}/api/auth/providers"))
        .send()
        .await
        .map_err(unreachable(origin))?;
    if !response.status().is_success() {
        return Err(refused(response).await);
    }
    response.json().await.map_err(|_| Error::Other(format!("{} is not a Layer server.", host(origin))))
}

/// The session a sign-in answered with, held here.
async fn hold(origin: &str, company: String, response: reqwest::Response) -> Result<SignedIn> {
    if !response.status().is_success() {
        return Err(refused(response).await);
    }
    let token = response
        .headers()
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| Error::Other(format!("{} gave no session for Layer on a Mac.", host(origin))))?;
    let me: Me = response.json().await?;
    let shown =
        SignedIn { host: host(origin), company, email: me.user.email.clone(), workspaces: me.workspaces.clone() };
    *PENDING.lock() = Some(Signed { origin: origin.into(), token, me });
    Ok(shown)
}

/// Sign in with the server's company in the browser (PROF-17), which comes
/// back on `layer://signed-in` with a code only this copy's secret can trade.
pub async fn sign_in_browser(app: &AppHandle, origin: &str) -> Result<SignedIn> {
    let company = match ways(origin).await? {
        Ways { oidc: true, company } => company.unwrap_or_else(|| host(origin)),
        _ => return Err(Error::Other(format!("{} signs nobody in.", host(origin)))),
    };
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = hex::encode(Sha256::digest(secret.as_bytes()));
    let mut url = Url::parse(&format!("{origin}/api/auth/oidc")).map_err(|e| Error::Other(e.to_string()))?;
    url.query_pairs_mut().append_pair("desktop", &challenge);

    let turn = TURN.fetch_add(1, Ordering::Relaxed);
    let (sender, back) = oneshot::channel();
    *WAITING.lock() = Some((turn, sender));
    let _ = take_relayed(&relay_path());
    app.opener().open_url(url.as_str(), None::<&str>).map_err(|e| Error::Other(e.to_string()))?;
    let back = wait(back).await;
    {
        let mut waiting = WAITING.lock();
        if waiting.as_ref().is_some_and(|(t, _)| *t == turn) {
            *waiting = None;
        }
    }
    let code = back.map_err(|problem| Error::Other(problem_words(&problem, &company)))?;

    profiles::bring_forward(std::process::id() as i32);
    let response = crate::integrations::http_client()
        .post(format!("{origin}/api/auth/oidc/desktop"))
        .header(CLIENT_HEADER, "desktop")
        .json(&json!({ "code": code, "secret": secret }))
        .send()
        .await
        .map_err(unreachable(origin))?;
    hold(origin, company, response).await
}

/// Stop waiting for the browser.
pub fn cancel() {
    WAITING.lock().take();
}

/// The code, from this copy's deep link or another's relay, or why not.
async fn wait(mut back: oneshot::Receiver<Back>) -> Back {
    let relay = relay_path();
    let until = tokio::time::Instant::now() + WAIT;
    loop {
        tokio::select! {
            came = &mut back => return came.unwrap_or_else(|_| Err("cancelled".into())),
            () = tokio::time::sleep(Duration::from_millis(300)) => {}
        }
        let relay = relay.clone();
        if let Ok(Some(came)) = tauri::async_runtime::spawn_blocking(move || take_relayed(&relay)).await {
            return came;
        }
        if tokio::time::Instant::now() >= until {
            return Err("too_slow".into());
        }
    }
}

fn relay_path() -> PathBuf {
    profiles::current().base.join(RELAY)
}

/// `layer://signed-in` reached this copy (PROF-17): for its own sign-in if
/// it waits for one, otherwise left for the copy that does.
pub fn arrived(urls: &[Url]) {
    for url in urls.iter().filter(|u| u.scheme() == "layer" && u.host_str() == Some("signed-in")) {
        let came = back_of(url);
        if let Some((_, sender)) = WAITING.lock().take() {
            let _ = sender.send(came);
            continue;
        }
        let relayed = match came {
            Ok(code) => json!({ "code": code, "at": now() }),
            Err(why) => json!({ "error": why, "at": now() }),
        };
        let _ = std::fs::write(relay_path(), relayed.to_string());
    }
}

fn take_relayed(relay: &Path) -> Option<Back> {
    let raw = std::fs::read_to_string(relay).ok()?;
    let _ = std::fs::remove_file(relay);
    let relayed: Value = serde_json::from_str(&raw).ok()?;
    if now() - relayed["at"].as_i64().unwrap_or(0) > WAIT.as_secs() as i64 {
        return None;
    }
    match (relayed["code"].as_str(), relayed["error"].as_str()) {
        (Some(code), _) => Some(Ok(code.to_string())),
        (_, Some(why)) => Some(Err(why.to_string())),
        _ => None,
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn back_of(url: &Url) -> Back {
    let query = |name: &str| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.to_string());
    match (query("code"), query("error")) {
        (Some(code), _) => Ok(code),
        (_, Some(problem)) => Err(problem),
        _ => Err(String::new()),
    }
}

/// What a failed sign-in with `company` means, as the web app says it.
fn problem_words(problem: &str, company: &str) -> String {
    match problem {
        "cancelled" => "Signing in was cancelled.".into(),
        "too_slow" => "Signing in took longer than five minutes, so Layer stopped waiting. Try again.".into(),
        "oidc_not_set_up" => "Signing in isn't set up on this server.".into(),
        "oidc_expired" => "That took too long. Try again.".into(),
        "oidc_other_browser" => "Start signing in from Layer, and finish in the browser it opened.".into(),
        "oidc_refused" => format!("{company} didn't sign you in. Try again."),
        "oidc_no_email" => format!("Your {company} account has no email address Layer can use."),
        "oidc_no_access" => format!("You don't have access to Layer yet. Ask {company}'s admin to add you."),
        "oidc_taken" => format!("The Layer account with your email is linked to another {company} account."),
        _ => "Something went wrong. Try again.".into(),
    }
}

/// Open workspace `id` from the sign-in held here, in a window of its own
/// (PROF-15): the profile that has it already, or a new one named as the
/// server names it. Blocks: call it off the runtime.
pub fn open(id: &str) -> Result<()> {
    let (origin, token, workspace, user) = {
        let pending = PENDING.lock();
        let signed = pending.as_ref().ok_or_else(|| Error::Other("Sign in to the server again.".into()))?;
        let workspace = signed
            .me
            .workspaces
            .iter()
            .find(|w| w.id.to_string() == id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("workspace {id}")))?;
        (signed.origin.clone(), signed.token.clone(), workspace, signed.me.user.id)
    };
    let me = profiles::current();
    let known = profiles::read(&me.base)
        .profiles
        .into_iter()
        .find(|p| p.server.as_deref() == Some(origin.as_str()) && p.workspace.as_deref() == Some(id));
    let profile = match known {
        Some(p) => p.id,
        None => make(&me.base, &origin, &workspace, user)?,
    };
    if profile == me.id {
        return Ok(());
    }
    match profiles::holder(&profiles::folder_of(&me.base, &profile)) {
        Some(pid) => {
            profiles::bring_forward(pid);
            Ok(())
        }
        None => {
            crate::secrets::seed(&me.service_for(&profile), crate::secrets::SESSION, &token)?;
            profiles::launch(&profile)
        }
    }
}

/// A profile for the server's workspace, with its `config.json` already
/// saying where the workspace is, so it never starts a Postgres.
fn make(base: &Path, origin: &str, workspace: &WorkspaceInfo, user: Uuid) -> Result<String> {
    let entry = profiles::change(base, |r| {
        let name = r
            .check_name(&workspace.name, None)
            .or_else(|_| r.check_name(&format!("{} ({})", workspace.name, host(origin)), None))?;
        let mut entry = Entry::new(r.new_id(&name), name);
        entry.server = Some(origin.to_string());
        entry.workspace = Some(workspace.id.to_string());
        r.profiles.push(entry.clone());
        Ok(entry)
    })?;
    let folder = profiles::folder_of(base, &entry.id);
    std::fs::create_dir_all(&folder)?;
    let at = WorkspaceAt {
        place: origin.to_string(),
        id: workspace.id.to_string(),
        user_id: Some(user.to_string()),
        admin: workspace.role == Role::Admin,
    };
    let cfg = crate::config::AppConfig {
        version: crate::config::SCHEMA_VERSION,
        shared: crate::config::shared::Shared { workspace: Some(at), ..Default::default() },
        ..Default::default()
    };
    std::fs::write(folder.join("config.json"), serde_json::to_vec_pretty(&cfg)?)?;
    Ok(entry.id)
}

/// Where a server workspace is, with the session in its keychain item.
pub async fn place(at: WorkspaceAt) -> std::result::Result<Place, String> {
    let token = tauri::async_runtime::spawn_blocking(|| crate::secrets::get(crate::secrets::SESSION))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| SIGNED_OUT.to_string())?;
    Ok(Place { origin: at.place, token, user_id: at.user_id.unwrap_or_default(), workspace_id: at.id })
}

/// After signing in again (PROF-16): the sign-in held here, if it is the
/// same person on the same server and they are still in the workspace,
/// becomes this workspace's session. Blocks: call it off the runtime.
pub fn resume(at: &WorkspaceAt) -> Result<Place> {
    let pending = PENDING.lock();
    let signed = pending
        .as_ref()
        .filter(|s| s.origin == at.place)
        .ok_or_else(|| Error::Other(format!("Sign in to {} first.", host(&at.place))))?;
    if at.user_id.as_ref().is_some_and(|u| *u != signed.me.user.id.to_string()) {
        return Err(Error::Other(
            "Another account opened this workspace on this Mac. Sign in with it, or open yours from New workspace."
                .into(),
        ));
    }
    if !signed.me.workspaces.iter().any(|w| w.id.to_string() == at.id) {
        return Err(Error::Other("You're no longer a member of this workspace.".into()));
    }
    crate::secrets::set(crate::secrets::SESSION, &signed.token)?;
    Ok(Place {
        origin: at.place.clone(),
        token: signed.token.clone(),
        user_id: signed.me.user.id.to_string(),
        workspace_id: at.id.clone(),
    })
}

/// The workspace's members page on its server (PROF-18).
pub async fn members_page(place: &Place) -> Result<String> {
    let key = mine(place).await?.url_key;
    Ok(format!("{}/{key}/settings/members", place.origin))
}

/// Whether the person is the workspace's admin now, as its server says (SET-7).
pub async fn admin(place: &Place) -> Result<bool> {
    Ok(mine(place).await?.role == Role::Admin)
}

/// The workspace, as the server tells the person in it.
async fn mine(place: &Place) -> Result<WorkspaceInfo> {
    let response = crate::integrations::http_client()
        .get(format!("{}/api/me", place.origin))
        .bearer_auth(&place.token)
        .send()
        .await
        .map_err(unreachable(&place.origin))?;
    if !response.status().is_success() {
        return Err(refused(response).await);
    }
    let me: Me = response.json().await?;
    me.workspaces
        .into_iter()
        .find(|w| w.id.to_string() == place.workspace_id)
        .ok_or_else(|| Error::Other("You're no longer a member of this workspace.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_read_as_its_origin_over_https_except_on_this_mac() {
        let read = |a: &str| read_address(a).ok();
        assert_eq!(read("layer.codevillain.eu").as_deref(), Some("https://layer.codevillain.eu"));
        assert_eq!(read(" https://layer.codevillain.eu/ ").as_deref(), Some("https://layer.codevillain.eu"));
        assert_eq!(read("localhost:7680").as_deref(), Some("http://localhost:7680"));
        assert_eq!(read("http://layer.codevillain.eu"), None);
        assert_eq!(read("not a server"), None);
        for origin in ["https://layer.codevillain.eu", "http://localhost:7680"] {
            assert_eq!(read(&host(origin)).as_deref(), Some(origin), "a host reads back as its server");
        }
    }

    #[test]
    fn a_sign_in_comes_back_as_a_code_or_a_problem() {
        let back = |u: &str| back_of(&Url::parse(u).unwrap());
        assert_eq!(back("layer://signed-in?code=c0de&next=%2F"), Ok("c0de".into()));
        assert_eq!(back("layer://signed-in?error=oidc_no_access"), Err("oidc_no_access".into()));
        assert_eq!(
            problem_words("oidc_no_access", "Acme Inc"),
            "You don't have access to Layer yet. Ask Acme Inc's admin to add you."
        );
        assert_eq!(problem_words("new_problem", "Acme Inc"), problem_words("", "Acme Inc"));
    }

    #[test]
    fn a_relayed_sign_in_is_taken_once_and_not_when_stale() {
        let dir = std::env::temp_dir().join(format!("vl-relay-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let relay = dir.join(RELAY);
        std::fs::write(&relay, json!({ "code": "c0de", "at": now() }).to_string()).unwrap();
        assert_eq!(take_relayed(&relay), Some(Ok("c0de".into())));
        assert_eq!(take_relayed(&relay), None);
        std::fs::write(&relay, json!({ "code": "old", "at": now() - 600 }).to_string()).unwrap();
        assert_eq!(take_relayed(&relay), None);
    }
}
