//! Connecting a personal token to one GitHub source without changing others.

use layer_domain::Validate;
use tauri::{AppHandle, Runtime};

use crate::config::github::{GithubCredential, GithubScope, GithubSource};
use crate::config::{AppConfig, GithubConfig};
use crate::error::{Error, Result};
use crate::git::GithubOrigin;
use crate::integrations::github::{self, GitHub};

use super::{blocking, AppState};

pub(crate) fn client_for(cfg: &AppConfig, key: &str) -> Result<(GitHub, GithubConfig)> {
    let source = cfg.github_source(key).ok_or_else(|| Error::Other(format!("Add {key} in Settings → Code.")))?;
    let token = token_for(cfg, key, &source.config)?;
    Ok((GitHub::new(&source.config, &token)?, source.config))
}

pub(crate) fn client_for_repo(cfg: &AppConfig, remote: &GithubOrigin) -> Result<(GitHub, String)> {
    let key = &remote.host;
    let source = cfg.github_source(key).ok_or_else(|| Error::Other(format!("Add {key} in Settings → Code.")))?;
    let web = layer_domain::github_address(&source.config.web_url).map_err(|e| Error::Other(e.0))?;
    let web = web.origin().ascii_serialization();
    if remote.web_origin.as_ref().is_some_and(|origin| origin != &web) {
        return Err(Error::Other(format!("This repository's origin does not match the GitHub web address for {key}.")));
    }
    let token = token_for(cfg, key, &source.config)?;
    Ok((GitHub::new(&source.config, &token)?, web))
}

/// A write must not use a stale origin from the background PR cache.
pub(crate) async fn checkout_client(state: &AppState, path: &str) -> Result<(GithubOrigin, GitHub)> {
    let cfg = state.config.read();
    let dir = std::path::PathBuf::from(path);
    super::off_runtime(move || {
        let mut origin = crate::git::github_origin(&dir)?;
        let (client, web) = client_for_repo(&cfg, &origin)?;
        origin.web_origin = Some(web);
        Ok((origin, client))
    })
    .await?
}

pub(crate) fn verify_pull_url(expected: &str, actual: &str) -> Result<()> {
    let expected = layer_domain::github_address(expected).ok();
    let actual = layer_domain::github_address(actual).ok();
    if expected.zip(actual).is_some_and(|(a, b)| {
        a.origin() == b.origin() && a.path().trim_end_matches('/').eq_ignore_ascii_case(b.path().trim_end_matches('/'))
    }) {
        return Ok(());
    }
    Err(Error::Other(
        "This pull request's source or repository changed. Refresh the pull requests and try again.".into(),
    ))
}

fn token_for(cfg: &AppConfig, key: &str, config: &GithubConfig) -> Result<String> {
    let not_connected = || Error::Other(format!("Connect {key} in Settings → Code."));
    let binding = cfg.github_sources.credentials.get(key).ok_or_else(not_connected)?;
    if binding.origin != config.api_origin()? {
        return Err(Error::Other(format!(
            "The API address for {key} changed. Enter its token again in Settings → Code."
        )));
    }
    binding.read(key)?.ok_or_else(not_connected)
}

fn normalized(api_url: &str, web_url: &str, review_team: Option<&str>) -> Result<GithubConfig> {
    let url = |value: &str| {
        layer_domain::github_address(value.trim())
            .map(|u| u.as_str().trim_end_matches('/').to_string())
            .map_err(|e| Error::Other(e.0))
    };
    let settings = layer_domain::GithubSettings {
        api_url: url(api_url)?,
        web_url: url(web_url)?,
        review_team: review_team.and_then(github::normalize_review_team),
    };
    settings.validate().map_err(|e| Error::Other(e.0))?;
    Ok(GithubConfig { api_url: settings.api_url, web_url: settings.web_url, review_team: settings.review_team })
}

fn edit(
    cfg: &AppConfig,
    source: Option<&str>,
    next: &GithubConfig,
) -> Result<(Option<GithubSource>, GithubConfig, String)> {
    let was = source
        .map(|key| cfg.github_source(key).ok_or_else(|| Error::NotFound(format!("GitHub source {key}"))))
        .transpose()?;
    let next = match &was {
        Some(was) if was.shared && cfg.shared.workspace.as_ref().is_some_and(|w| !w.is_admin()) => was.config.clone(),
        _ => next.clone(),
    };
    let key = next.source_key().ok_or_else(|| Error::Other("Enter a GitHub web URL with a host.".into()))?;
    if source != Some(&key) && cfg.github_source(&key).is_some() {
        return Err(Error::Other(format!("{key} is already a GitHub source.")));
    }
    Ok((was, next, key))
}

pub(crate) async fn connect<R: Runtime>(
    app: AppHandle<R>,
    source: Option<String>,
    api_url: &str,
    web_url: &str,
    token: String,
    review_team: Option<&str>,
) -> Result<String> {
    let next = normalized(api_url, web_url, review_team)?;
    let typed = !token.trim().is_empty();
    let (was, next, key, token) = {
        let source = source.clone();
        blocking(app.clone(), move |state| {
            let cfg = state.config.read();
            let (was, next, key) = edit(&cfg, source.as_deref(), &next)?;
            let token = if typed {
                token.trim().to_string()
            } else if source.as_deref() == Some(&key) {
                token_for(&cfg, &key, &next)?
            } else {
                return Err(Error::Other(format!("Enter a token for {key}.")));
            };
            Ok((was, next, key, token))
        })
        .await?
    };
    let login = match GitHub::new(&next, &token)?.login().await {
        Err(Error::TokenRejected(_)) if typed => Err(Error::Other(format!("{key} did not accept this token."))),
        other => other,
    }?;
    blocking(app, move |state| save(state, &key, was, next, &token, typed)).await?;
    Ok(login)
}

fn save(
    state: &AppState,
    key: &str,
    was: Option<GithubSource>,
    next: GithubConfig,
    token: &str,
    typed: bool,
) -> Result<()> {
    // Credential changes, config writes and publishing results take turns
    // (`NativeGitHub`): a read that began before this cannot land after it.
    let mut session = state.github_session.blocking_lock();
    // A token typed in gets a fresh slot. If the config write fails or a
    // workspace update superseded the form, the old binding and credential
    // are still usable. An edit that keeps the token keeps its slot, and so
    // the source's revision: a new review team is not a reconnect, and what
    // Reviews has already seen stays seen.
    let kept = if typed { None } else { state.config.with(|c| c.github_sources.credentials.get(key).cloned()) };
    let binding = match kept.clone() {
        Some(binding) => binding,
        None => {
            let binding = GithubCredential::new(&next)?;
            binding.save(key, token)?;
            binding
        }
    };
    let changed = session.update(state, typed, || {
        state.config.update_github(|c| {
            let (now, checked, _) = edit(c, was.as_ref().map(|g| g.key.as_str()), &next)?;
            if now != was || checked != next {
                return Err(Error::Other("The GitHub settings changed while connecting. Try again.".into()));
            }
            let old = match kept {
                Some(_) => None,
                None => was
                    .as_ref()
                    .and_then(|g| c.github_sources.credentials.remove(&g.key).map(|binding| (g.key.clone(), binding))),
            };
            c.set_github_source(was.as_ref(), next);
            c.github_sources.credentials.insert(key.to_string(), binding.clone());
            Ok(old)
        })
    });
    match changed {
        Ok(old) => {
            if let Some((key, binding)) = old {
                binding.delete(&key)?;
            }
            Ok(())
        }
        Err(e) => {
            if kept.is_none() {
                let _ = binding.delete(key);
            }
            Err(e)
        }
    }
}

pub(crate) fn disconnect(state: &AppState, key: &str) -> Result<()> {
    let mut session = state.github_session.blocking_lock();
    let removed = session.update(state, true, || {
        state.config.update_github(|c| {
            let source = c.github_source(key).ok_or_else(|| Error::NotFound(format!("GitHub source {key}")))?;
            if source.scope == GithubScope::Local {
                c.github_sources.local.retain(|g| g.source_key().as_deref() != Some(key));
            }
            Ok(c.github_sources.credentials.remove(key))
        })
    })?;
    if let Some(binding) = removed {
        binding.delete(key)?;
    }
    // GitHub issues may have been the tracker: its ticket baseline goes too.
    state.news.forget_tickets();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_repositories_with_the_same_slug_use_their_own_api_and_token() {
        use axum::{
            http::{HeaderMap, StatusCode},
            response::IntoResponse,
            routing::get,
            Json, Router,
        };

        let mut cfg = AppConfig::default();
        let mut slots = Vec::new();
        let mut servers = Vec::new();
        for (host, label, token) in
            [("github.com", "public", "fake-public"), ("git.example.com", "private", "fake-private")]
        {
            let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let config =
                normalized(&format!("http://{}", socket.local_addr().unwrap()), &format!("https://{host}"), None)
                    .unwrap();
            let cred = GithubCredential::new(&config).unwrap();
            cred.save(host, token).unwrap();
            cfg.github_sources.credentials.insert(host.into(), cred.clone());
            cfg.github_sources.local.push(config);
            slots.push((host, cred));
            servers.push(tokio::spawn(async move {
                axum::serve(
                    socket,
                    Router::new().route(
                        "/user",
                        get(move |headers: HeaderMap| async move {
                            if headers.get("authorization").and_then(|h| h.to_str().ok())
                                != Some(&format!("Bearer {token}"))
                            {
                                return StatusCode::UNAUTHORIZED.into_response();
                            }
                            Json(serde_json::json!({ "login": label })).into_response()
                        }),
                    ),
                )
                .await
                .unwrap();
            }));
        }
        for (url, expected) in
            [("https://github.com/acme/web", "public"), ("ssh://git@git.example.com:2222/acme/web", "private")]
        {
            let remote = GithubOrigin::parse(url).unwrap();
            assert_eq!(client_for_repo(&cfg, &remote).unwrap().0.login().await.unwrap(), expected);
        }
        for url in ["https://git.example.com:9999/acme/web", "https://git.example.com.evil.test/acme/web"] {
            assert!(client_for_repo(&cfg, &GithubOrigin::parse(url).unwrap()).is_err());
        }
        for server in servers {
            server.abort();
        }
        for (key, credential) in slots {
            credential.delete(key).unwrap();
        }
    }

    #[test]
    fn a_legacy_token_moves_once_and_other_hosts_keep_theirs() {
        let legacy = normalized("https://legacy.example.com/api/v3", "https://legacy.example.com", None).unwrap();
        let other =
            GithubCredential::new(&normalized("https://api.github.com", "https://github.com", None).unwrap()).unwrap();
        let mut cfg = AppConfig { github: Some(legacy.clone()), ..Default::default() };
        crate::secrets::set(crate::secrets::GITHUB, "fake-legacy-token").unwrap();
        other.save("github.com", "fake-public-token").unwrap();
        cfg.migrate_github();
        assert_eq!(token_for(&cfg, "legacy.example.com", &legacy).unwrap(), "fake-legacy-token");
        assert!(crate::secrets::get(crate::secrets::GITHUB).unwrap().is_none());
        // Restarting from the same upgraded config works with the old slot gone.
        let cfg: AppConfig = serde_json::from_value(serde_json::to_value(cfg).unwrap()).unwrap();
        assert_eq!(token_for(&cfg, "legacy.example.com", &legacy).unwrap(), "fake-legacy-token");
        cfg.github_sources.credentials["legacy.example.com"].delete("legacy.example.com").unwrap();
        assert_eq!(other.read("github.com").unwrap().as_deref(), Some("fake-public-token"));
        other.delete("github.com").unwrap();
    }

    #[tokio::test]
    async fn another_api_origin_receives_no_github_token_on_redirect() {
        use axum::{http::HeaderMap, response::Redirect, routing::get, Json, Router};

        let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let probe = received.clone();
        let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let to = format!("http://{}/user", other.local_addr().unwrap());
        let receiver = tokio::spawn(async move {
            let router = Router::new().route(
                "/user",
                get(move |headers: HeaderMap| {
                    probe.store(headers.contains_key("authorization"), std::sync::atomic::Ordering::SeqCst);
                    async { Json(serde_json::json!({ "login": "not-the-source" })) }
                }),
            );
            axum::serve(other, router).await.unwrap();
        });
        let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cfg = GithubConfig { api_url: format!("http://{}", source.local_addr().unwrap()), ..Default::default() };
        let redirect = tokio::spawn(async move {
            axum::serve(source, Router::new().route("/user", get(move || async move { Redirect::temporary(&to) })))
                .await
                .unwrap();
        });
        let login = GitHub::new(&cfg, "fake-token").unwrap().login().await;
        receiver.abort();
        redirect.abort();
        assert!(login.unwrap_err().to_string().contains("redirected"));
        assert!(!received.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn a_credential_is_not_reused_after_an_origin_change() {
        let before = normalized("https://git.example.com/api/v3", "https://git.example.com", None).unwrap();
        let mut cfg = AppConfig::default();
        let binding = GithubCredential::new(&before).unwrap();
        binding.save("git.example.com", "fake-enterprise-token").unwrap();
        cfg.github_sources.credentials.insert("git.example.com".into(), binding.clone());
        let same = normalized("https://GIT.example.com:443/other", "https://git.example.com", None).unwrap();
        assert_eq!(token_for(&cfg, "git.example.com", &same).unwrap(), "fake-enterprise-token");
        let other = normalized("https://git.example.com:4430/api/v3", "https://git.example.com", None).unwrap();
        assert!(token_for(&cfg, "git.example.com", &other).is_err());
        assert!(token_for(&cfg, "github.com", &same).is_err());
        binding.delete("git.example.com").unwrap();
    }
}
