use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};

/// An app the workspace watches. Raven checks its health address every half
/// minute, and takes the calls and logs it sends with its key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct App {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// What the app calls itself in OpenTelemetry (`service.name`), unique
    /// in the workspace: `forge`. It can't change.
    pub key: String,
    pub name: String,
    /// Up when it answers 2xx. None for an app that only sends calls, and
    /// for Raven itself, which the server checks at its own address.
    pub health_url: Option<String>,
    /// Where it sits in the list.
    pub position: i32,
    /// Raven itself, which records its own calls here. Set by the server.
    pub is_self: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct AppCreate {
    pub key: String,
    pub name: String,
    pub health_url: Option<String>,
    /// After the last app when left out.
    pub position: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct AppPatch {
    pub name: Option<String>,
    /// `null` stops the health checks.
    pub health_url: Option<String>,
    pub position: Option<i32>,
}

/// A new ingest key, shown once: the app sends its calls with
/// `Authorization: Bearer <key>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppKey {
    pub key: String,
}

impl Validate for AppCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        service_name(&self.key)?;
        name(&self.name)?;
        self.health_url.as_deref().map(health_url).transpose()?;
        Ok(())
    }
}

impl Validate for AppPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.name.as_deref().map(name).transpose()?;
        self.health_url.as_deref().map(health_url).transpose()?;
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> Result<(), ValidationError> {
    Err(ValidationError(message.into()))
}

fn name(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return invalid("An app needs a name.");
    }
    if value.chars().count() > 100 {
        return invalid("That name is longer than 100 characters.");
    }
    Ok(())
}

/// Lowercase letters, digits and dashes, as services name themselves.
fn service_name(value: &str) -> Result<(), ValidationError> {
    let valid = (1..=64).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-');
    if !valid {
        return invalid("A service name is lowercase letters, digits and dashes, like forge.");
    }
    // `/<workspace>/settings` is the workspace's settings, not an app.
    if value == "settings" {
        return invalid("\"settings\" can't be a service name in Raven.");
    }
    Ok(())
}

/// An http or https address Raven can call. Loopback and private hosts are
/// fine (This Mac watches local apps); link-local and cloud-metadata are not.
fn health_url(value: &str) -> Result<(), ValidationError> {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    let Some(rest) = rest.filter(|rest| !rest.is_empty() && !rest.starts_with('/')) else {
        return invalid("A health address starts with http:// or https://.");
    };
    if value.len() > 500 || value.contains(char::is_whitespace) {
        return invalid("A health address starts with http:// or https://.");
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match authority.split_once(']') {
        Some((v6, _)) => v6.strip_prefix('[').unwrap_or(v6),
        None => authority.split(':').next().unwrap_or_default(),
    };
    if host.is_empty() || !health_host_ok(host) {
        return invalid("A health address can't be a link-local or metadata address.");
    }
    Ok(())
}

fn health_host_ok(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => !link_local_or_metadata(ip),
        Err(_) => {
            host != "metadata.google.internal"
                && !host.ends_with(".metadata.google.internal")
        }
    }
}

fn link_local_or_metadata(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            // 169.254.0.0/16 is link-local; cloud metadata sits in it too.
            ip.is_link_local() || (a == 169 && b == 254)
        }
        std::net::IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(v4) => link_local_or_metadata(std::net::IpAddr::V4(v4)),
            None => ip.is_unicast_link_local(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_names_are_what_otel_services_call_themselves() {
        for good in ["forge", "glyph-dev", "app2"] {
            assert!(service_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "Forge",
            "-forge",
            "for ge",
            "forge_dev",
            "settings",
            &"a".repeat(65),
        ] {
            assert!(service_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn health_addresses_are_http() {
        assert!(health_url("http://127.0.0.1:7385/api/health").is_ok());
        assert!(health_url("https://forge.codevillain.eu/api/health").is_ok());
        for bad in [
            "",
            "forge.codevillain.eu",
            "ftp://x",
            "http://",
            "http:///x",
            "http://a b",
            "http://169.254.169.254/latest/meta-data",
            "http://metadata.google.internal/",
        ] {
            assert!(health_url(bad).is_err(), "{bad}");
        }
    }
}
