use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError, invalid, text};

/// Logs kept in S3 that agents read: where they are. The AWS profile that
/// reads them is each person's own, signed in by their `aws` CLI, so it
/// stays on their Mac.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogSource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// What agents name it by: "Production request logs".
    pub name: String,
    /// The profile's own when None.
    pub region: Option<String>,
    pub bucket: String,
    /// Agents read only under it. Empty, or ending in `/`.
    pub prefix: String,
    pub athena: Option<AthenaTable>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The Athena table over a log source's files, for querying them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AthenaTable {
    pub workgroup: String,
    pub database: String,
    pub table: String,
    /// The most data its queries may scan in a day, in GB. None: the app's default.
    pub daily_limit_gb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LogSourceCreate {
    pub name: String,
    pub region: Option<String>,
    pub bucket: String,
    pub prefix: Option<String>,
    pub athena: Option<AthenaTable>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LogSourcePatch {
    pub name: Option<String>,
    pub region: Option<String>,
    pub bucket: Option<String>,
    pub prefix: Option<String>,
    /// `null` stops queries.
    pub athena: Option<AthenaTable>,
}

impl Validate for LogSourceCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.name, "A log source's name", 100)?;
        bucket(&self.bucket)?;
        self.prefix.as_deref().map(prefix).transpose()?;
        Ok(())
    }
}

impl Validate for LogSourcePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.name
            .as_deref()
            .map(|n| text(n, "A log source's name", 100))
            .transpose()?;
        self.bucket.as_deref().map(bucket).transpose()?;
        self.prefix.as_deref().map(prefix).transpose()?;
        Ok(())
    }
}

/// S3's own rule for bucket names, loosely: lowercase, digits, dots and dashes.
fn bucket(value: &str) -> Result<(), ValidationError> {
    let valid = (3..=63).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    if valid {
        Ok(())
    } else {
        invalid("A bucket name is lowercase letters, digits, dots and dashes.")
    }
}

fn prefix(value: &str) -> Result<(), ValidationError> {
    if value.is_empty() || (value.ends_with('/') && !value.starts_with('/')) {
        Ok(())
    } else {
        invalid("A prefix ends in /, like logs/requests/.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_is_a_folder() {
        assert!(prefix("").is_ok());
        assert!(prefix("logs/requests/").is_ok());
        assert!(prefix("logs").is_err());
        assert!(prefix("/logs/").is_err());
    }
}
