//! The orders tickets can be listed in (TKT-13), and the site's own order of
//! priorities, which is what "Priority" sorts by.

use serde_json::Value;

use super::{str_at, Jira};
use crate::error::Result;

/// How a search asks Jira to order its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Most recently changed first: what every search did before there was a choice.
    Updated,
    /// Most recently filed first.
    Newest,
    Oldest,
    /// Highest first, in the order the site ranks its priorities.
    Priority,
}

impl Order {
    pub fn parse(s: &str) -> Self {
        match s {
            "newest" => Order::Newest,
            "oldest" => Order::Oldest,
            "priority" => Order::Priority,
            _ => Order::Updated,
        }
    }

    /// The search is capped, so the order is Jira's to apply: sorting the
    /// first hundred here would only reshuffle the hundred most recently
    /// updated, not find the hundred highest. Ties fall back to recent work.
    pub(super) fn clause(self) -> &'static str {
        match self {
            Order::Updated => "ORDER BY updated DESC",
            Order::Newest => "ORDER BY created DESC",
            Order::Oldest => "ORDER BY created ASC",
            Order::Priority => "ORDER BY priority DESC, updated DESC",
        }
    }
}

impl Jira {
    /// Every priority's name, highest first, as the site lists them.
    ///
    /// Names are a site's own ("P1", "Blocker", "Highest"), so no order
    /// can be written down here; the site's list is the only ranking.
    pub async fn priorities(&self) -> Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        // A site has a handful; the cap only stops a runaway.
        while out.len() < 500 {
            let v = self
                .json(self.req(
                    reqwest::Method::GET,
                    &format!("/rest/api/3/priority/search?startAt={}&maxResults=100", out.len()),
                ))
                .await?;
            let page = names(&v);
            if page.is_empty() {
                break;
            }
            out.extend(page);
            if v.get("isLast").and_then(Value::as_bool).unwrap_or(true) {
                break;
            }
        }
        Ok(out)
    }
}

fn names(v: &Value) -> Vec<String> {
    v.get("values")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|p| str_at(p, "name")).filter(|n| !n.is_empty()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_unknown_order_falls_back_to_recently_updated() {
        assert_eq!(Order::parse("sideways"), Order::Updated);
        assert_eq!(Order::parse("priority"), Order::Priority);
    }

    #[test]
    fn priorities_keep_the_order_the_site_lists_them_in() {
        let v = json!({ "values": [{ "name": "Blocker" }, { "name": "" }, { "name": "Minor" }], "isLast": true });
        assert_eq!(names(&v), vec!["Blocker".to_string(), "Minor".to_string()]);
    }
}
