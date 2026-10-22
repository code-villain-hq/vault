//! Sending each app its people and groups, as Pocket ID's SCIM sync does:
//! list what the app has, then create, replace or delete each user, then
//! each group. Every few seconds, so an app that starts later, or starts
//! over with an empty database, gets them too.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::{APPS, DevId, People, SECRET};

const USER: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";

pub async fn keep_pushing(dev: Arc<DevId>) {
    let client = reqwest::Client::new();
    // What each app was last sent, and whether it was up.
    let mut last: HashMap<&str, Option<People>> = HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        let people = match dev.people() {
            Ok(people) => people,
            Err(e) => {
                eprintln!("dev-id: {e}");
                continue;
            }
        };
        for (app, name, base) in APPS {
            match push(&client, app, base, &people).await {
                Ok(()) => {
                    if last.get(app).cloned().flatten().as_ref() != Some(&people) {
                        println!(
                            "dev-id: {name} has {} people of {}",
                            people.of(app).len(),
                            people.company
                        );
                    }
                    last.insert(app, Some(people.clone()));
                }
                // Not running: nothing to say until it is.
                Err(e) if e.is::<reqwest::Error>() => {
                    last.insert(app, None);
                }
                Err(e) => {
                    if last.get(app).cloned().flatten().is_some() {
                        eprintln!("dev-id: {name} refused its people: {e:#}");
                    }
                    last.insert(app, None);
                }
            }
        }
    }
}

async fn push(
    client: &reqwest::Client,
    app: &str,
    base: &str,
    people: &People,
) -> anyhow::Result<()> {
    let scim = format!("{base}/api/scim/v2");
    let call = |method: reqwest::Method, path: &str| {
        client
            .request(method, format!("{scim}{path}"))
            .bearer_auth(SECRET)
            .header("content-type", "application/scim+json")
            .timeout(Duration::from_secs(5))
    };
    let list = |path: &'static str| {
        let request = call(
            reqwest::Method::GET,
            &format!("{path}?startIndex=1&count=1000"),
        );
        async move {
            let response = request.send().await?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                anyhow::bail!("its SCIM isn't on (set its SCIM token to dev)");
            }
            let body: Value = response.error_for_status()?.json().await?;
            let mut known = HashMap::new();
            for r in body["Resources"].as_array().into_iter().flatten() {
                if let (Some(id), Some(external)) = (r["id"].as_str(), r["externalId"].as_str()) {
                    known.insert(external.to_string(), id.to_string());
                }
            }
            anyhow::Ok(known)
        }
    };

    let send = |request: reqwest::RequestBuilder, body: Value| async move {
        let response = request.json(&body).send().await?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            anyhow::bail!("{status}: {}", body["detail"].as_str().unwrap_or(""));
        }
        anyhow::Ok(body["id"].as_str().unwrap_or_default().to_string())
    };

    let members = people.of(app);
    let users = list("/Users").await?;
    let mut ids: HashMap<String, String> = HashMap::new();
    for person in &members {
        let body = json!({
            "schemas": [USER],
            "externalId": person.id,
            "userName": person.id,
            "displayName": person.name,
            "active": true,
            "emails": [{ "value": person.email, "primary": true }],
        });
        let id = match users.get(&person.id) {
            Some(id) => {
                send(call(reqwest::Method::PUT, &format!("/Users/{id}")), body).await?;
                id.clone()
            }
            None => send(call(reqwest::Method::POST, "/Users"), body).await?,
        };
        ids.insert(person.id.clone(), id);
    }
    for (external, id) in &users {
        if !ids.contains_key(external) {
            call(reqwest::Method::DELETE, &format!("/Users/{id}"))
                .send()
                .await?
                .error_for_status()?;
        }
    }

    // Every group of the app's people, its own and the rest (Forge links
    // teams to groups such as design-users).
    let mut groups: Vec<&str> = members
        .iter()
        .flat_map(|p| p.groups.iter().map(String::as_str))
        .collect();
    groups.sort();
    groups.dedup();
    let known = list("/Groups").await?;
    for group in &groups {
        let in_it: Vec<Value> = members
            .iter()
            .filter(|p| p.groups.iter().any(|g| g == group))
            .filter_map(|p| ids.get(&p.id))
            .map(|id| json!({ "value": id }))
            .collect();
        let body = json!({
            "schemas": [GROUP],
            "externalId": group,
            "displayName": group,
            "members": in_it,
        });
        match known.get(*group) {
            Some(id) => send(call(reqwest::Method::PUT, &format!("/Groups/{id}")), body).await?,
            None => send(call(reqwest::Method::POST, "/Groups"), body).await?,
        };
    }
    for (external, id) in &known {
        if !groups.contains(&external.as_str()) {
            call(reqwest::Method::DELETE, &format!("/Groups/{id}"))
                .send()
                .await?
                .error_for_status()?;
        }
    }
    Ok(())
}
