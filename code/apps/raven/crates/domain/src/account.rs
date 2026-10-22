//! Accounts, workspaces and membership: the request and response types of
//! the account API, and the rules for names and emails.
//! The same as Forge's, without agents and GitHub.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::ValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Role {
    /// Manages the workspace: members and the apps it watches.
    Admin,
    Member,
    /// Sees only the apps they've been added to.
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
    /// The workspace's name in URLs, e.g. `studio` in `/studio/forge`.
    pub url_key: String,
    pub role: Role,
}

/// The signed-in user and their workspaces.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Me {
    pub user: UserInfo,
    pub workspaces: Vec<WorkspaceInfo>,
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
#[ts(export)]
pub struct MemberInfo {
    pub user: UserInfo,
    pub role: Role,
    pub joined_at: DateTime<Utc>,
    /// The apps they've been added to. Only a guest's matter: admins and
    /// members see them all.
    pub access: Vec<Uuid>,
    /// The company's group their role comes from, such as `raven-admins`:
    /// it's changed in the company's account, not in Members.
    pub from_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateMemberRequest {
    pub role: Role,
}

/// Puts a guest in exactly these apps.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateAccessRequest {
    pub items: Vec<Uuid>,
}

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

/// Trims a name and checks it isn't empty or too long.
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
    fn url_keys_are_readable_and_avoid_app_paths() {
        assert_eq!(url_key_from("Code Villain"), "code-villain");
        assert_eq!(url_key_from("  Evil!!  Lair "), "evil-lair");
        assert_eq!(url_key_from("Über"), "ber");
        assert_eq!(url_key_from("???"), "workspace");
        assert_eq!(url_key_from("Login"), "login-team");
        assert_eq!(url_key_from("Signed in"), "signed-in-team");
        assert!(url_key_from(&"a".repeat(80)).len() <= 32);
    }
}
