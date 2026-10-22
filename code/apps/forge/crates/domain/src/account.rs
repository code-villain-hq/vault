//! Accounts, workspaces, membership and invites: the request and response
//! types of the account API, and the rules for names, emails and passwords.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::ValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Role {
    /// Manages the workspace: members, invites and settings.
    Admin,
    Member,
    /// Sees only the teams they've been added to, and works there as a
    /// member does.
    Guest,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Guest => "guest",
        }
    }

    pub fn parse(value: &str) -> Option<Role> {
        match value {
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            "guest" => Some(Role::Guest),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

/// A workspace the signed-in user belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceInfo {
    pub id: Uuid,
    pub name: String,
    /// The workspace's name in URLs, e.g. `forge` in `/forge/issues`.
    pub url_key: String,
    pub role: Role,
}

/// The signed-in user and their workspaces.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Me {
    pub user: UserInfo,
    pub workspaces: Vec<WorkspaceInfo>,
    /// Whether you can sign in with a password (people who joined with
    /// GitHub start without one).
    pub has_password: bool,
    /// Whether a GitHub account is connected, to sign in with.
    pub github_connected: bool,
}

/// Creates an account. With `workspace_name` it also creates a workspace;
/// with `invite_token` it joins the invite's workspace instead.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct SignupRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub workspace_name: Option<String>,
    pub invite_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Renames a workspace. Its URL key stays, so links keep working.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateWorkspaceRequest {
    pub name: String,
}

/// Changes your own profile.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateProfileRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemberInfo {
    pub user: UserInfo,
    pub role: Role,
    pub joined_at: DateTime<Utc>,
    /// The teams they're in.
    pub access: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateMemberRequest {
    pub role: Role,
}

/// Sets the teams someone is in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateAccessRequest {
    pub items: Vec<Uuid>,
}

/// An active invite link, as admins see it. The token itself is shown once,
/// when the link is created.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Invite {
    pub id: Uuid,
    pub role: Role,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateInviteRequest {
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreatedInvite {
    pub invite: Invite,
    /// Goes in the link: `/join/<token>`.
    pub token: String,
}

/// What anyone with an invite link may see before joining.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InvitePreview {
    pub workspace_name: String,
    /// Where to go once joined.
    pub workspace_url_key: String,
    pub invited_by: String,
    /// What the invite makes you there.
    pub role: Role,
}

pub const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 256;
const MAX_NAME: usize = 100;

/// Trims and lowercases an email address and checks it looks like one.
pub fn normalize_email(email: &str) -> Result<String, ValidationError> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && !email.contains(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    if valid {
        Ok(email)
    } else {
        Err(ValidationError(
            "that doesn't look like an email address".into(),
        ))
    }
}

/// Where agents' addresses are: `agent-<id>@agents.forge.invalid` for agents
/// added in Settings, `<name>@<workspace>.agents.forge.invalid` for Forge's
/// own. `.invalid` can't receive mail.
pub const AGENT_EMAIL_DOMAIN: &str = "agents.forge.invalid";

/// Whether an address is one Forge gives agents, which people can't use:
/// its domain is [`AGENT_EMAIL_DOMAIN`] or ends in `.agents.forge.invalid`.
pub fn is_agent_email(email: &str) -> bool {
    let email = email.trim().to_lowercase();
    email.rsplit_once('@').is_some_and(|(_, domain)| {
        let domain = domain.trim_end_matches('.');
        domain == AGENT_EMAIL_DOMAIN
            || domain
                .strip_suffix(AGENT_EMAIL_DOMAIN)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

pub fn check_password(password: &str) -> Result<(), ValidationError> {
    let length = password.chars().count();
    if length < MIN_PASSWORD {
        return Err(ValidationError(format!(
            "password must be at least {MIN_PASSWORD} characters"
        )));
    }
    if length > MAX_PASSWORD {
        return Err(ValidationError(format!(
            "password must be at most {MAX_PASSWORD} characters"
        )));
    }
    Ok(())
}

/// Trims a person's or workspace's name and checks its length.
pub fn clean_name(name: &str, what: &str) -> Result<String, ValidationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ValidationError(format!("{what} can't be empty")));
    }
    if name.chars().count() > MAX_NAME {
        return Err(ValidationError(format!(
            "{what} is longer than {MAX_NAME} characters"
        )));
    }
    Ok(name.to_string())
}

/// Paths the app uses itself, which a workspace's URL key must not take.
const RESERVED_URL_KEYS: &[&str] = &[
    "api",
    "assets",
    "join",
    "login",
    "logout",
    "mcp",
    "settings",
    "signed-in",
    "signup",
    "start",
];

/// A URL key based on a workspace name: lowercase letters, digits and dashes.
pub fn url_key_from(name: &str) -> String {
    let mut key = String::new();
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch);
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    let key: String = key.trim_end_matches('-').chars().take(32).collect();
    let key = key.trim_end_matches('-').to_string();
    if key.is_empty() {
        "workspace".into()
    } else if RESERVED_URL_KEYS.contains(&key.as_str()) {
        format!("{key}-team")
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_trimmed_and_lowercased() {
        assert_eq!(
            normalize_email("  Ada@Example.COM ").unwrap(),
            "ada@example.com"
        );
        for bad in [
            "",
            "ada",
            "@example.com",
            "ada@",
            "ada@example",
            "a da@example.com",
            "ada@.com",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn agents_addresses_are_told_apart() {
        for agent in [
            "agent-0192@agents.forge.invalid",
            "github@0192-abcd.agents.forge.invalid",
            " Forge-AI@0192.Agents.Forge.Invalid ",
            "x@agents.forge.invalid.",
            "a@b@agents.forge.invalid",
        ] {
            assert!(is_agent_email(agent), "{agent:?} is an agent's");
        }
        for person in [
            "ada@example.com",
            "ada@myagents.forge.invalid",
            "ada@agents.forge.invalid.example.com",
            "agents.forge.invalid@example.com",
        ] {
            assert!(!is_agent_email(person), "{person:?} is a person's");
        }
    }

    #[test]
    fn passwords_need_a_minimum_length() {
        assert!(check_password("1234567").is_err());
        assert!(check_password("12345678").is_ok());
        assert!(check_password(&"x".repeat(257)).is_err());
    }

    #[test]
    fn url_keys_are_readable_and_avoid_app_paths() {
        assert_eq!(url_key_from("Code Villain"), "code-villain");
        assert_eq!(url_key_from("  Forge!!  Team "), "forge-team");
        assert_eq!(url_key_from("Über"), "ber");
        assert_eq!(url_key_from("???"), "workspace");
        assert_eq!(url_key_from("Login"), "login-team");
        assert_eq!(url_key_from("MCP"), "mcp-team");
        assert_eq!(url_key_from("Signed in"), "signed-in-team");
        assert_eq!(url_key_from("Start"), "start-team");
        assert!(url_key_from(&"a".repeat(80)).len() <= 32);
    }
}

/// An agent in the workspace and its tokens, as members see it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentInfo {
    pub id: Uuid,
    pub name: String,
    pub tokens: Vec<ApiTokenInfo>,
    /// Where Forge wakes the agent when there's work for it, if anywhere.
    pub runner: Option<RunnerInfo>,
}

/// An agent's runner: the URL Forge POSTs wake-ups to, and how the latest
/// ones went.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunnerInfo {
    pub url: String,
    pub created_at: DateTime<Utc>,
    /// The latest wake-ups, newest first.
    pub wakeups: Vec<WakeupInfo>,
}

/// One wake-up sent, or to be sent, to an agent's runner.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WakeupInfo {
    pub id: Uuid,
    pub event: WakeupEvent,
    /// The issue's identifier, e.g. FOR-12.
    pub issue: String,
    pub status: WakeupStatus,
    pub attempts: i32,
    /// Why the last attempt failed, e.g. "HTTP 500" or "couldn't connect".
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

/// Why an agent is woken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WakeupEvent {
    /// An issue was assigned to it.
    Assigned,
    /// Someone else commented on an issue assigned to it.
    Comment,
    /// An issue assigned to it moved back, e.g. from In Review to In Progress.
    SentBack,
    /// Someone mentioned it.
    Mention,
    /// An issue assigned to it was accepted out of Triage into planned or
    /// started work, e.g. after it asked for a decision there.
    Accepted,
    /// The pull request for an issue assigned to it ran into conflicts or
    /// failing checks: merge the default branch in, or fix the checks.
    PrBlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WakeupStatus {
    /// Held while the agent is busy, or until it has taken up the last one.
    Waiting,
    /// Not delivered yet; retried until the runner answers or Forge gives up.
    Pending,
    Delivered,
    /// Forge gave up after several attempts.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SetRunnerRequest {
    pub url: String,
}

/// The secret a runner checks wake-ups with. Shown once, when it's made.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunnerSecret {
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiTokenInfo {
    pub id: Uuid,
    /// The token's last four characters, so people can tell their keys apart.
    /// None for tokens made before hints were kept.
    pub hint: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateAgentRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateAgentRequest {
    pub name: String,
}

/// A new token. The token itself is shown once, when it's created.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreatedToken {
    pub agent_id: Uuid,
    pub token: String,
}
