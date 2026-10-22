//! The GitHub App's OAuth client (`github.client_id` and
//! `github.client_secret`): with it, installing the App confirms that the
//! person who installed it can reach the installation (`super::setup`).
//! Nobody signs in with GitHub: people sign in with the company's account.

use std::time::Duration;

use serde_json::Value;

pub struct GitHubOAuth {
    client_id: String,
    client_secret: String,
    /// github.com, or a stand-in in tests.
    web: String,
    api: String,
    client: reqwest::Client,
}

impl GitHubOAuth {
    /// From `github.client_id` and `github.client_secret`; `None` without them.
    pub fn from_settings(settings: &crate::config::Settings) -> Option<Self> {
        Some(Self::new(
            settings.get("github.client_id")?,
            settings.get("github.client_secret")?,
            "https://github.com".into(),
            "https://api.github.com".into(),
        ))
    }

    pub fn new(client_id: String, client_secret: String, web: String, api: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("forge")
            .build()
            .expect("an HTTP client with default settings builds");
        GitHubOAuth {
            client_id,
            client_secret,
            web,
            api,
            client,
        }
    }

    async fn access_token(&self, code: &str) -> Option<String> {
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
            .ok()?
            .json()
            .await
            .ok()?;
        response["access_token"].as_str().map(str::to_string)
    }

    async fn get(&self, path: &str, token: &str) -> Option<Value> {
        let response = self
            .client
            .get(format!("{}{path}", self.api))
            .bearer_auth(token)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            tracing::warn!(%path, status = %response.status(), "GitHub refused a request");
            return None;
        }
        response.json().await.ok()
    }

    /// Whether the person GitHub gave this OAuth `code` to can access the
    /// installation: GitHub lists the app's installations each person can
    /// access. `None` if GitHub didn't answer, or refused the code.
    pub async fn can_access_installation(&self, code: &str, installation_id: i64) -> Option<bool> {
        const PER_PAGE: usize = 100;
        let token = self.access_token(code).await?;
        let mut page = 1;
        loop {
            let path = format!("/user/installations?per_page={PER_PAGE}&page={page}");
            let body = self.get(&path, &token).await?;
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
}

/// `GET /api/auth/github/callback`: the App's callback URL. With "Request
/// user authorization (OAuth) during installation" on, GitHub sends whoever
/// installed it here instead of to the setup URL, with the code that says
/// who they are; it goes on to setup.
pub async fn callback(
    axum::extract::RawQuery(query): axum::extract::RawQuery,
) -> axum::response::Response {
    use axum::response::{IntoResponse, Redirect};
    let query = query.unwrap_or_default();
    if query.split('&').any(|p| p.starts_with("installation_id=")) {
        Redirect::to(&format!("/api/github/setup?{query}")).into_response()
    } else {
        axum::http::StatusCode::NOT_FOUND.into_response()
    }
}
