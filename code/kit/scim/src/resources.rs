//! SCIM's wire format: what comes in, and what goes back.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserIn {
    pub external_id: Option<String>,
    pub user_name: String,
    pub name: Option<NameIn>,
    pub display_name: Option<String>,
    pub active: Option<bool>,
    #[serde(default)]
    pub emails: Vec<EmailIn>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameIn {
    pub given_name: Option<String>,
    pub family_name: Option<String>,
}

#[derive(Deserialize)]
pub struct EmailIn {
    pub value: String,
    #[serde(default)]
    pub primary: bool,
}

impl UserIn {
    /// The name to show: the display name, else given and family name, else the user name.
    pub fn shown_name(&self) -> String {
        let full = self
            .name
            .as_ref()
            .map(|n| {
                [&n.given_name, &n.family_name]
                    .into_iter()
                    .flatten()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        [
            self.display_name.as_deref().unwrap_or_default(),
            &full,
            &self.user_name,
        ]
        .into_iter()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("New member")
        .chars()
        .take(100)
        .collect()
    }

    /// The primary email, else the first, in lower case.
    pub fn email(&self) -> Option<String> {
        self.emails
            .iter()
            .find(|e| e.primary)
            .or(self.emails.first())
            .map(|e| e.value.trim().to_lowercase())
            .filter(|e| e.contains('@'))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupIn {
    pub external_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub members: Vec<MemberIn>,
}

#[derive(Deserialize)]
pub struct MemberIn {
    pub value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    start_index: Option<i64>,
    count: Option<i64>,
}

impl ListQuery {
    /// SCIM counts from 1.
    pub fn start(&self) -> i64 {
        self.start_index.unwrap_or(1).max(1)
    }

    pub fn offset(&self) -> i64 {
        self.start() - 1
    }

    pub fn limit(&self) -> i64 {
        self.count.unwrap_or(100).clamp(0, 1000)
    }
}

pub struct UserRow {
    pub id: Uuid,
    pub external_id: String,
    pub user_name: String,
    pub name: String,
    pub email: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

pub struct GroupRow {
    pub id: Uuid,
    pub external_id: Option<String>,
    pub name: String,
    pub members: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

// `meta` leaves out `lastModified`: without it Pocket ID sends every person and
// group again on each sync, so a change it made without touching their own
// time still arrives. That's a few dozen small requests an hour.

pub fn user(user: &UserRow) -> Value {
    json!({
        "schemas": [USER_SCHEMA],
        "id": user.id,
        "externalId": user.external_id,
        "userName": user.user_name,
        "displayName": user.name,
        "active": user.active,
        "emails": [{ "value": user.email, "primary": true }],
        "meta": { "resourceType": "User", "created": user.created_at },
    })
}

pub fn group(group: &GroupRow) -> Value {
    json!({
        "schemas": [GROUP_SCHEMA],
        "id": group.id,
        "externalId": group.external_id,
        "displayName": group.name,
        "members": group.members.iter().map(|id| json!({ "value": id })).collect::<Vec<_>>(),
        "meta": { "resourceType": "Group", "created": group.created_at },
    })
}

pub fn list(total: i64, start: i64, resources: Vec<Value>) -> Value {
    json!({
        "schemas": [LIST_SCHEMA],
        "totalResults": total,
        "startIndex": start,
        "itemsPerPage": resources.len(),
        "Resources": resources,
    })
}

pub fn service_provider_config() -> Value {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
        "patch": { "supported": false },
        "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
        "filter": { "supported": false, "maxResults": 1000 },
        "changePassword": { "supported": false },
        "sort": { "supported": false },
        "etag": { "supported": false },
        "authenticationSchemes": [{
            "type": "oauthbearertoken",
            "name": "Bearer token",
            "description": "The token set as <APP>_SCIM_TOKEN on the server",
        }],
    })
}
