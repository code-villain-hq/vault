//! How much of each agent plan's limits is used, and when each window
//! starts over (§27). Claude Code hands its own to a status line the app
//! installs (LIMIT-2); the ChatGPT plan OpenCode signs in with is asked of
//! OpenAI with that sign-in (LIMIT-3). The latest of each is kept in
//! `limits.json`, so the corner has something to say before the first
//! agent of the day has (LIMIT-4).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

/// One window of a plan: how much of it is used, and when it starts over.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// From 0 to 100.
    pub used_percent: f64,
    /// Epoch milliseconds; None when the provider did not say.
    pub resets_at: Option<i64>,
}

/// What one provider last said of its plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanLimits {
    /// `claude` or `openai`.
    pub provider: String,
    /// The short window: five hours, for both.
    pub session: Option<Window>,
    /// The week.
    pub weekly: Option<Window>,
    /// When it was read, epoch milliseconds.
    pub read_at: i64,
}

/// The latest of each provider, and where they are kept.
pub struct Limits {
    path: PathBuf,
    latest: parking_lot::Mutex<BTreeMap<String, PlanLimits>>,
}

impl Limits {
    /// What `dir`'s `limits.json` holds. A missing or unreadable file is
    /// nothing yet: it is only the last reading, and the next replaces it.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("limits.json");
        let kept: Vec<PlanLimits> =
            std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let latest = kept.into_iter().map(|l| (l.provider.clone(), l)).collect();
        Self { path, latest: parking_lot::Mutex::new(latest) }
    }

    /// Every provider's latest, in a steady order.
    pub fn list(&self) -> Vec<PlanLimits> {
        self.latest.lock().values().cloned().collect()
    }

    /// Keep `limits` as its provider's latest, and write the file. Under the
    /// lock, so two writes never cross.
    fn take(&self, limits: PlanLimits) {
        let mut latest = self.latest.lock();
        latest.insert(limits.provider.clone(), limits);
        let all: Vec<&PlanLimits> = latest.values().collect();
        if let Ok(bytes) = serde_json::to_vec_pretty(&all) {
            if let Err(e) = crate::agents::replace_file(&self.path, &bytes, None) {
                eprintln!("limits.json could not be written: {e}");
            }
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn keep(app: &AppHandle, limits: PlanLimits) {
    let store = app.state::<Limits>();
    store.take(limits);
    let _ = app.emit("limits:changed", store.list());
}

// ---------------------------------------------------------------------------
// Claude Code (LIMIT-2)

/// How often a pane's status line may post. Claude Code runs it after every
/// change, 300ms apart at the most, while a reply streams; the numbers it
/// carries move far slower than that.
const STATUS_LINE_EVERY_SECS: u32 = 15;

/// The status line Claude Code runs: post what it was handed to the app,
/// at most every `STATUS_LINE_EVERY_SECS` per pane, and only when it carries
/// the plan's limits (an API key's session has none). It prints nothing, so
/// the pane shows no status line of its own, and always exits 0.
pub fn status_line_command() -> String {
    format!(
        "input=$(cat); [ -n \"$VILLAIN_HOOK_URL\" ] && [ -n \"$VILLAIN_PANE\" ] || exit 0; \
         case \"$input\" in *'\"rate_limits\"'*) ;; *) exit 0 ;; esac; \
         stamp=\"${{TMPDIR:-/tmp}}/villain-limits-$VILLAIN_PANE\"; now=$(date +%s); \
         last=$(cat \"$stamp\" 2>/dev/null); case \"$last\" in ''|*[!0-9]*) last=0 ;; esac; \
         [ $((now - last)) -lt {STATUS_LINE_EVERY_SECS} ] && exit 0; echo \"$now\" > \"$stamp\"; \
         printf '%s' \"$input\" | curl -s -m 2 -o /dev/null -X POST \
         -H \"Authorization: Bearer $VILLAIN_HOOK_TOKEN\" -H 'Content-Type: application/json' \
         --data-binary @- \"${{VILLAIN_HOOK_URL%/hook}}/limits/claude\"; exit 0"
    )
}

/// Whether the user has a status line of their own. The app's settings file
/// is laid over theirs, so installing one would take theirs away.
pub fn user_has_status_line() -> bool {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".claude")));
    dir.and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| owns_status_line(&v))
}

/// Whether these settings hold a status line that is the user's. Orca's is
/// not: it installs one in the user's settings that stays after Orca is put
/// away, and outside a pane Orca started (no `ORCA_*` variables) it reads
/// its input and prints nothing, so in the app's panes it shows nothing to
/// take away.
fn owns_status_line(settings: &Value) -> bool {
    let Some(line) = settings.get("statusLine").filter(|s| !s.is_null()) else { return false };
    let command = line.get("command").and_then(Value::as_str).unwrap_or_default();
    !command.contains(".orca/agent-hooks/claude-statusline")
}

/// What a status line post says of the plan, or None when it says nothing.
/// The windows are documented as `used_percentage` and `resets_at` in epoch
/// seconds; the usage endpoint's `utilization`, milliseconds and dates are
/// read too, so a rename shows as the same numbers rather than none.
pub fn from_status_line(payload: &Value, read_at: i64) -> Option<PlanLimits> {
    let limits = payload.get("rate_limits")?;
    let window = |key: &str| {
        let w = limits.get(key)?;
        let used = w.get("used_percentage").or_else(|| w.get("utilization")).and_then(Value::as_f64)?;
        Some(Window { used_percent: used.clamp(0.0, 100.0), resets_at: epoch_ms(w.get("resets_at")) })
    };
    let (session, weekly) = (window("five_hour"), window("seven_day"));
    (session.is_some() || weekly.is_some()).then(|| PlanLimits { provider: "claude".into(), session, weekly, read_at })
}

/// A post from a pane's status line.
pub fn take_status_line(app: &AppHandle, payload: &Value) {
    if let Some(limits) = from_status_line(payload, now_ms()) {
        keep(app, limits);
    }
}

/// A time as epoch milliseconds, from seconds, milliseconds (anything past
/// 1e10, which as seconds is the year 2286) or an RFC 3339 date.
fn epoch_ms(v: Option<&Value>) -> Option<i64> {
    let n = match v? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => match s.trim().parse::<f64>() {
            Ok(n) => n,
            Err(_) => return chrono::DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.timestamp_millis()),
        },
        _ => return None,
    };
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    Some(if n > 1e10 { n as i64 } else { (n * 1000.0) as i64 })
}

// ---------------------------------------------------------------------------
// OpenAI, for OpenCode signed in with ChatGPT (LIMIT-3)

/// How often OpenAI is asked while an OpenCode agent is running.
const OPENAI_EVERY: Duration = Duration::from_secs(5 * 60);

const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// The ChatGPT sign-in OpenCode keeps: only what asking needs.
#[derive(Debug, PartialEq)]
struct SignIn {
    access: String,
    account: Option<String>,
    /// Epoch milliseconds.
    expires: Option<i64>,
}

/// OpenCode's data folder: `$XDG_DATA_HOME/opencode`, as it places it.
fn opencode_data() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".local/share")))
        .map(|d| d.join("opencode"))
}

/// OpenCode's ChatGPT sign-in, read and never changed: refreshing it would
/// hand OpenCode's refresh token a new one OpenCode does not know of, and
/// sign it out. It keeps it in `opencode.db` now, read with the system's
/// `sqlite3` read-only and through a pipe, never on a command line; before
/// that, in `auth.json`.
fn opencode_sign_in() -> Option<SignIn> {
    let data = opencode_data()?;
    let db = data.join("opencode.db");
    if db.exists() {
        let out = std::process::Command::new("/usr/bin/sqlite3")
            .arg("-readonly")
            .arg(&db)
            .arg("select value from credential where integration_id = 'openai' and active = 1 limit 1")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if out.status.success() {
            if let Some(sign_in) = sign_in_from(&String::from_utf8_lossy(&out.stdout)) {
                return Some(sign_in);
            }
        }
    }
    let auth: Value = serde_json::from_str(&std::fs::read_to_string(data.join("auth.json")).ok()?).ok()?;
    sign_in_from(&auth.get("openai")?.to_string())
}

/// A sign-in from OpenCode's record of it. Only an OAuth one has a plan to
/// read; an API key is billed by use and has no window.
fn sign_in_from(raw: &str) -> Option<SignIn> {
    let v: Value = serde_json::from_str(raw.trim()).ok()?;
    if v.get("type")?.as_str()? != "oauth" {
        return None;
    }
    let access = v.get("access")?.as_str()?.trim().to_string();
    let account =
        v.pointer("/metadata/accountID").or_else(|| v.get("accountId")).and_then(Value::as_str).map(str::to_string);
    (!access.is_empty()).then(|| SignIn { access, account, expires: v.get("expires").and_then(Value::as_i64) })
}

/// What OpenAI's answer says of the plan. Of its two windows, the one a day
/// long or shorter is the session's; without lengths, the first is.
fn from_chatgpt(body: &Value, read_at: i64) -> Option<PlanLimits> {
    let windows = body.get("rate_limit")?;
    let (mut session, mut weekly) = (None, None);
    for w in [windows.get("primary_window"), windows.get("secondary_window")].into_iter().flatten() {
        let Some(used) = w.get("used_percent").and_then(Value::as_f64) else { continue };
        let window = Window { used_percent: used.clamp(0.0, 100.0), resets_at: epoch_ms(w.get("reset_at")) };
        let long = w.get("limit_window_seconds").and_then(Value::as_f64).is_some_and(|s| s > 86_400.0);
        if long || session.is_some() {
            weekly = Some(window);
        } else {
            session = Some(window);
        }
    }
    (session.is_some() || weekly.is_some()).then(|| PlanLimits { provider: "openai".into(), session, weekly, read_at })
}

/// Ask OpenAI how much of the ChatGPT plan is used. None, saying why on
/// stderr, when there is no sign-in, it has run out (OpenCode renews it the
/// next time it runs) or the answer cannot be read.
async fn ask_openai() -> Option<PlanLimits> {
    let sign_in = tauri::async_runtime::spawn_blocking(opencode_sign_in).await.ok()??;
    if sign_in.expires.is_some_and(|e| e <= now_ms()) {
        return None;
    }
    let mut request = crate::integrations::http_client()
        .get(CHATGPT_USAGE_URL)
        .bearer_auth(&sign_in.access)
        .timeout(Duration::from_secs(10));
    if let Some(account) = &sign_in.account {
        request = request.header("ChatGPT-Account-Id", account);
    }
    let response = match request.send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            eprintln!("ChatGPT usage: {}", r.status());
            return None;
        }
        Err(e) => {
            eprintln!("ChatGPT usage: {e}");
            return None;
        }
    };
    from_chatgpt(&response.json::<Value>().await.ok()?, now_ms())
}

/// Ask OpenAI now, keeping what it says.
pub async fn refresh_openai(app: &AppHandle) {
    if let Some(limits) = ask_openai().await {
        keep(app, limits);
    }
}

/// Ask OpenAI once at start, then every `OPENAI_EVERY` while an OpenCode
/// agent is running: nothing else spends the plan.
pub fn spawn_openai_watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        refresh_openai(&app).await;
        loop {
            tokio::time::sleep(OPENAI_EVERY).await;
            let busy = app.try_state::<crate::commands::AppState>().is_some_and(|s| {
                s.ptys.list(None).iter().any(|p| p.running && p.agent_id.as_deref() == Some("opencode"))
            });
            if busy {
                refresh_openai(&app).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_status_line_post_reads_both_windows_as_documented() {
        let post = json!({
            "model": {"display_name": "Opus"},
            "rate_limits": {
                "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
                "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}
            }
        });
        let read = from_status_line(&post, 7).unwrap();
        assert_eq!(read.provider, "claude");
        assert_eq!(read.session, Some(Window { used_percent: 23.5, resets_at: Some(1_738_425_600_000) }));
        assert_eq!(read.weekly, Some(Window { used_percent: 41.2, resets_at: Some(1_738_857_600_000) }));
        assert_eq!(read.read_at, 7);
    }

    #[test]
    fn a_status_line_without_limits_says_nothing_and_a_dropped_window_is_none() {
        // An API key's session, or one before its first reply.
        assert_eq!(from_status_line(&json!({"model": {}}), 0), None);
        assert_eq!(from_status_line(&json!({"rate_limits": {}}), 0), None);
        // Claude Code drops a window once it has reset.
        let one = from_status_line(&json!({"rate_limits": {"seven_day": {"used_percentage": 3}}}), 0).unwrap();
        assert_eq!(one.session, None);
        assert_eq!(one.weekly, Some(Window { used_percent: 3.0, resets_at: None }));
    }

    #[test]
    fn a_renamed_or_differently_timed_window_still_reads() {
        let post = json!({"rate_limits": {"five_hour": {"utilization": 140, "resets_at": "2026-11-11T21:00:00Z"}}});
        let read = from_status_line(&post, 0).unwrap().session.unwrap();
        assert_eq!(read.used_percent, 100.0, "never past full");
        assert_eq!(read.resets_at, Some(1_794_430_800_000));
        assert_eq!(epoch_ms(Some(&json!(1_794_430_800_000_i64))), Some(1_794_430_800_000), "already milliseconds");
        assert_eq!(epoch_ms(Some(&json!("1794430800"))), Some(1_794_430_800_000));
        assert_eq!(epoch_ms(Some(&json!(null))), None);
    }

    #[test]
    fn the_status_line_posts_to_the_limits_route_and_never_fails() {
        let cmd = status_line_command();
        assert!(cmd.contains("${VILLAIN_HOOK_URL%/hook}/limits/claude"));
        assert!(cmd.contains("--data-binary @-") && cmd.ends_with("exit 0"));
        assert!(cmd.contains("$VILLAIN_HOOK_TOKEN") && !cmd.contains("VILLAIN_MCP_TOKEN"));
    }

    #[test]
    #[cfg(unix)]
    fn the_status_line_posts_once_per_window_and_only_with_limits() {
        // Run it for real, with curl swapped for a recorder.
        let dir = std::env::temp_dir().join(format!("vl-status-line-{}", uuid::Uuid::new_v4()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let posts = dir.join("posts");
        let fake = bin.join("curl");
        std::fs::write(&fake, format!("#!/bin/sh\ncat >> '{}'; echo >> '{}'\n", posts.display(), posts.display()))
            .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let run = |input: &str| {
            use std::io::Write;
            let mut child = std::process::Command::new("sh")
                .arg("-c")
                .arg(status_line_command())
                .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                .env("TMPDIR", &dir)
                .env("VILLAIN_HOOK_URL", "http://127.0.0.1:1/hook")
                .env("VILLAIN_PANE", "pane-1")
                .env("VILLAIN_HOOK_TOKEN", "t")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success());
            assert!(out.stdout.is_empty(), "the pane shows nothing of it");
        };
        let lines = || std::fs::read_to_string(&posts).unwrap_or_default().lines().count();
        run(r#"{"model":{}}"#);
        assert_eq!(lines(), 0, "no limits, no post");
        run(r#"{"rate_limits":{"five_hour":{"used_percentage":1}}}"#);
        run(r#"{"rate_limits":{"five_hour":{"used_percentage":2}}}"#);
        assert_eq!(lines(), 1, "the second came within the window");
        std::fs::write(dir.join("villain-limits-pane-1"), "not a time").unwrap();
        run(r#"{"rate_limits":{"five_hour":{"used_percentage":3}}}"#);
        assert_eq!(lines(), 2, "a stamp it cannot read does not stop it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_status_line_of_the_users_own_is_kept_but_orcas_leftover_is_not() {
        assert!(!owns_status_line(&json!({"model": "opus"})));
        assert!(!owns_status_line(&json!({"statusLine": null})));
        assert!(owns_status_line(&json!({"statusLine": {"type": "command", "command": "~/bin/line.sh"}})));
        let orca = r#"if [ -f "${HOME-}/.orca/agent-hooks/claude-statusline.sh" ]; then /bin/sh "${HOME-}/.orca/agent-hooks/claude-statusline.sh"; fi"#;
        assert!(!owns_status_line(&json!({"statusLine": {"type": "command", "command": orca}})));
    }

    #[test]
    fn openai_windows_are_told_apart_by_their_length() {
        let body = json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {"used_percent": 12, "limit_window_seconds": 604800, "reset_at": 1794851878},
                "secondary_window": {"used_percent": 55, "limit_window_seconds": 18000, "reset_at": 1794430800}
            }
        });
        let read = from_chatgpt(&body, 1).unwrap();
        assert_eq!(read.provider, "openai");
        assert_eq!(read.session, Some(Window { used_percent: 55.0, resets_at: Some(1_794_430_800_000) }));
        assert_eq!(read.weekly, Some(Window { used_percent: 12.0, resets_at: Some(1_794_851_878_000) }));
        // Without lengths, primary then secondary.
        let bare =
            json!({"rate_limit": {"primary_window": {"used_percent": 1}, "secondary_window": {"used_percent": 2}}});
        let read = from_chatgpt(&bare, 1).unwrap();
        assert_eq!(read.session.unwrap().used_percent, 1.0);
        assert_eq!(read.weekly.unwrap().used_percent, 2.0);
        assert_eq!(from_chatgpt(&json!({"plan_type": "free", "rate_limit": null}), 1), None);
    }

    #[test]
    fn only_a_chatgpt_sign_in_is_taken_from_opencodes_record() {
        let oauth = r#"{"type":"oauth","methodID":"chatgpt-browser","access":"a1","refresh":"r1","expires":5,"metadata":{"accountID":"acc"}}"#;
        assert_eq!(
            sign_in_from(oauth),
            Some(SignIn { access: "a1".into(), account: Some("acc".into()), expires: Some(5) })
        );
        // OpenCode's older auth.json.
        let old = r#"{"type":"oauth","access":"a2","refresh":"r2","expires":6,"accountId":"acc2"}"#;
        assert_eq!(sign_in_from(old).unwrap().account.as_deref(), Some("acc2"));
        assert_eq!(sign_in_from(r#"{"type":"api","key":"sk-1"}"#), None, "a key has no plan");
        assert_eq!(sign_in_from(r#"{"type":"oauth","access":""}"#), None);
        assert_eq!(sign_in_from(""), None);
    }

    #[test]
    fn the_latest_of_each_provider_is_kept_across_a_restart() {
        let dir = std::env::temp_dir().join(format!("vl-limits-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let reading = |provider: &str, used: f64, at: i64| PlanLimits {
            provider: provider.into(),
            session: Some(Window { used_percent: used, resets_at: None }),
            weekly: None,
            read_at: at,
        };
        let limits = Limits::load(&dir);
        assert!(limits.list().is_empty());
        limits.take(reading("openai", 5.0, 1));
        limits.take(reading("claude", 10.0, 2));
        limits.take(reading("claude", 20.0, 3));
        let again = Limits::load(&dir);
        assert_eq!(again.list(), vec![reading("claude", 20.0, 3), reading("openai", 5.0, 1)]);
        std::fs::write(dir.join("limits.json"), "{ not json").unwrap();
        assert!(Limits::load(&dir).list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
