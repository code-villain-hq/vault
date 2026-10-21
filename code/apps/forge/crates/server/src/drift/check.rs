//! One drift check: build the context, run Claude with the code tools, and
//! report what happened.

use std::fmt::Write as _;

use forge_ai::{Agent, ModelProvider, Repo, Stop};
use forge_domain::{Actor, DecisionStatus, Issue, Service, StateType};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::DriftSettings;
use super::tools::{DriftTools, definitions};
use crate::mcp::snapshot::Snapshot;
use crate::mcp::tools::knowledge;

pub struct CheckResult {
    pub summary: String,
    pub error: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub suggestions: i32,
    pub messages: Vec<Value>,
}

const SYSTEM: &str = "\
You check one software service for drift: places where what the team's tracker says about it (its \
issues, specs, decisions and recorded current state) no longer matches what the code does. You can \
read the service's code and git history with the tools; you can't change code.

Drift to look for:
- Issues with the wrong status: marked done though the code doesn't do it, or still open though the \
code (or its git history) shows it's done.
- Specs that describe behaviour the code doesn't have, or miss important behaviour it does have.
- Decisions the code no longer follows (overridden), or that a later decision replaced.
- Changes in the code that matter to the team but that no issue, spec or decision mentions.

How to work:
- Start from the tracker context and the recent history below, then read code to verify. Never \
propose a change you haven't checked in the code, and cite files and line numbers as evidence.
- Use propose_change for each finding, with the concrete changes that would bring the tracker back in \
line with the code. A few well-founded suggestions beat many weak ones; if nothing drifted, propose \
nothing. People review every suggestion before anything changes.
- Long specs and decisions are cut off below; read them in full with read_spec or read_decision \
rather than judging what you haven't seen. The project and workspace-wide \
specs and decisions listed by title apply to the service too: read the ones that matter.
- Specs written as requirements (`### Requirement: <name>` with scenarios) list them in the context. \
Give every one of them a verdict with report_verdicts: met or not_met, citing files and lines, or \
unclear, saying why (e.g. the code that would do it is outside the service's paths). For each one \
not met, also propose_change, naming the spec and the requirement.
- Use update_current_state to record what the code does today: concise Markdown covering the main \
behaviours, interfaces and notable limits, at most about 400 words. That field is yours to keep \
accurate; write it even when nothing drifted.
- Finish with finish, summarising in a few sentences what you checked and what you found.";

pub async fn run(
    pool: &PgPool,
    provider: &dyn ModelProvider,
    settings: &DriftSettings,
    agent: Actor,
    run_id: Uuid,
    service_id: Uuid,
) -> Result<CheckResult, String> {
    let snapshot = Snapshot::load(pool, agent).await?;
    let service = snapshot
        .services
        .iter()
        .find(|s| s.id == service_id)
        .ok_or("the service was deleted")?
        .clone();
    let repository = service
        .repository
        .as_deref()
        .ok_or("the service has no repository")?;
    let path = settings.resolve_repository(repository)?;
    let repo = Repo::open(&path, &service.paths)
        .await
        .map_err(|e| e.to_string())?;
    if repo.file_count() == 0 {
        return Err("no tracked files match the service's paths".into());
    }
    let history = repo.log(None, 20).await.unwrap_or_default();
    let commit = repo.head().await.unwrap_or_default();
    let context = context(&snapshot, &service, repo.file_count(), &history);

    let mut tools = DriftTools::new(pool.clone(), agent, snapshot, service, repo, run_id, commit);
    let agent_loop = Agent {
        system: SYSTEM.into(),
        tools: definitions(),
        max_turns: settings.max_turns,
        max_tokens: 16_000,
    };
    let outcome = agent_loop
        .run(provider, context, &mut tools)
        .await
        .map_err(|e| e.to_string())?;

    let error = match &outcome.stop {
        Stop::Done => None,
        Stop::Refused(why) => Some(format!(
            "The model declined to continue{}",
            why.as_deref().map(|w| format!(": {w}")).unwrap_or_default()
        )),
        Stop::TurnLimit => Some(format!(
            "Stopped after {} turns without finishing.",
            settings.max_turns
        )),
        Stop::MaxTokens => Some("A response hit the output limit.".into()),
    };
    let summary = tools
        .summary
        .clone()
        .unwrap_or_else(|| last_text(&outcome.messages));
    Ok(CheckResult {
        summary,
        error,
        input_tokens: outcome.usage.input_tokens
            + outcome.usage.cache_creation_input_tokens
            + outcome.usage.cache_read_input_tokens,
        output_tokens: outcome.usage.output_tokens,
        cost_usd: outcome.usage.cost_usd(provider.model()),
        suggestions: tools.suggestions,
        messages: outcome.messages,
    })
}

/// The last thing the model said, for runs that ended without `finish`.
pub(super) fn last_text(messages: &[Value]) -> String {
    messages
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .find_map(|block| {
            (block["type"] == "text").then(|| block["text"].as_str().unwrap_or("").to_string())
        })
        .unwrap_or_default()
}

fn cut(text: &str, max: usize) -> String {
    cut_then(text, max, "")
}

/// `text` cut to `max` characters, saying how to read the rest.
fn cut_then(text: &str, max: usize, rest: &str) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}… (cut off{rest})")
}

/// What the tracker says about the service, for the first message.
fn context(s: &Snapshot, service: &Service, files: usize, history: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Service: {}\n", service.name);
    let _ = writeln!(
        out,
        "Repository: {} ({} tracked files in scope)",
        service.repository.as_deref().unwrap_or(""),
        files
    );
    if !service.paths.is_empty() {
        let _ = writeln!(out, "Paths: {}", service.paths.join(", "));
    }
    let _ = writeln!(
        out,
        "\n## What it's for\n\n{}",
        or_none(&service.description)
    );
    let _ = writeln!(
        out,
        "\n## Recorded current state{}\n\n{}",
        service
            .current_state_updated_at
            .map(|at| format!(" (as of {})", at.date_naive()))
            .unwrap_or_default(),
        or_none(&service.current_state)
    );

    let specs: Vec<_> = s
        .specs
        .iter()
        .filter(|sp| sp.service_ids.contains(&service.id))
        .collect();
    let _ = writeln!(out, "\n## Specs ({})", specs.len());
    for spec in specs {
        let _ = writeln!(
            out,
            "\n### {} [{:?}] (id {})\n\n{}",
            spec.title,
            spec.status,
            spec.id,
            cut_then(
                &spec.body,
                8000,
                &format!("; read it in full with read_spec \"{}\"", spec.id)
            )
        );
        let requirements = forge_domain::requirements(&spec.body);
        if !requirements.is_empty() {
            let _ = writeln!(
                out,
                "\nRequirements of \"{}\" to give a verdict on:",
                spec.title
            );
            for requirement in requirements {
                let _ = writeln!(out, "- {}", requirement.name);
            }
        }
    }

    let decisions: Vec<_> = s
        .decisions
        .iter()
        .filter(|d| d.service_id == Some(service.id))
        .collect();
    let _ = writeln!(out, "\n## Decisions ({})", decisions.len());
    for d in decisions {
        let read_decision = format!("; read it in full with read_decision \"{}\"", d.id);
        let status = match d.status {
            DecisionStatus::Proposed => "proposed",
            DecisionStatus::Accepted => "accepted",
            DecisionStatus::Superseded => "superseded",
            DecisionStatus::Overridden => "overridden",
        };
        let _ = writeln!(
            out,
            "\n### {} [{status}{}] (id {})\n\nContext: {}\n\nDecision: {}\n\nConsequences: {}",
            d.title,
            d.decided_on
                .map(|on| format!(", decided {on}"))
                .unwrap_or_default(),
            d.id,
            cut_then(&d.context, 3000, &read_decision),
            cut_then(&d.decision, 3000, &read_decision),
            cut_then(&d.consequences, 2000, &read_decision),
        );
    }

    let recent = chrono::Utc::now() - chrono::Duration::days(30);
    let issues: Vec<&Issue> = s
        .issues
        .iter()
        .filter(|i| i.service_id == Some(service.id))
        .filter(|i| i.completed_at.is_none_or(|at| at > recent))
        .collect();

    // What the issue pages of the service's issues show besides the
    // service's own: their projects' specs and decisions, and the
    // workspace-wide decisions. Titles only, to read when they matter.
    let mut projects: Vec<Uuid> = issues.iter().filter_map(|i| i.project_id).collect();
    projects.sort();
    projects.dedup();
    let (more_specs, more_decisions) = knowledge(s, Some(service.id), &projects);
    let more_specs: Vec<_> = more_specs
        .into_iter()
        .filter(|(_, via)| *via != "service")
        .collect();
    let more_decisions: Vec<_> = more_decisions
        .into_iter()
        .filter(|(_, via)| *via != "service")
        .collect();
    let _ = writeln!(
        out,
        "\n## Project and workspace-wide specs and decisions ({})\n",
        more_specs.len() + more_decisions.len()
    );
    let project_name = |id: Option<Uuid>| {
        id.and_then(|id| s.projects.iter().find(|p| p.id == id))
            .map_or("workspace-wide".to_string(), |p| {
                format!("project {}", p.name)
            })
    };
    for (spec, _) in &more_specs {
        let _ = writeln!(
            out,
            "- Spec \"{}\" ({}, id {})",
            spec.title,
            project_name(spec.project_id),
            spec.id
        );
    }
    for (d, _) in &more_decisions {
        let _ = writeln!(
            out,
            "- Decision \"{}\" ({}, id {})",
            d.title,
            project_name(d.project_id),
            d.id
        );
    }

    let _ = writeln!(
        out,
        "\n## Issues (open, and closed in the last 30 days: {})",
        issues.len()
    );
    for issue in issues {
        let state = s.state_of(issue);
        let closed =
            state.is_some_and(|st| matches!(st.kind, StateType::Completed | StateType::Canceled));
        let _ = writeln!(
            out,
            "\n### {} {} [{}{}]\n\n{}",
            s.identifier(issue),
            issue.title,
            state.map_or("?", |st| st.name.as_str()),
            if closed { ", closed" } else { "" },
            or_none(&cut(&issue.description, 1500)),
        );
    }
    if let Some(team) = service
        .team_id
        .and_then(|id| s.teams.iter().find(|t| t.id == id))
    {
        let mut states: Vec<_> = s.states.iter().filter(|st| st.team_id == team.id).collect();
        states.sort_by(|a, b| a.position.total_cmp(&b.position));
        let names: Vec<&str> = states.iter().map(|st| st.name.as_str()).collect();
        let _ = writeln!(
            out,
            "\nThe service's team is {} ({}); its statuses: {}.",
            team.name,
            team.key,
            names.join(", ")
        );
    }

    let _ = writeln!(
        out,
        "\n## Recent commits touching the service\n\n```\n{}\n```",
        cut(history, 6000)
    );
    out
}

fn or_none(text: &str) -> &str {
    if text.trim().is_empty() {
        "(none)"
    } else {
        text
    }
}
