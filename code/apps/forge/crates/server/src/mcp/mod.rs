//! The agent tool: an MCP server at `/mcp`, so coding agents read and write
//! the tracker, specs and decisions in Forge instead of copies in the repo.
//!
//! Agents authenticate with an API token (see `agents.rs`); the middleware
//! puts the agent's [`Actor`] on the request, and every tool acts as it.

pub mod drift_checks;
pub mod snapshot;
pub mod tools;

use std::sync::Arc;

use axum::Router;
use forge_domain::Actor;
use http::request::Parts;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use sqlx::PgPool;

use crate::agents;
use crate::state::AppState;
use drift_checks::{
    FinishDriftCheck, ReportDriftCheck, StartDriftCheck, finish_drift_check, report_drift_check,
    start_drift_check,
};
use snapshot::ToolResult;
use tools::*;

const INSTRUCTIONS: &str = "\
Forge is the team's issue tracker and the source of truth for specs and decisions. \
Read specs and decisions here instead of from files in the repository.

- Start with workspace_overview to learn the teams, statuses, people, projects and services.
- Before working on an issue, call get_issue: it includes the discussion, sub-issues, relations and \
the specs and decisions it follows (its service's, its project's and workspace-wide ones). \
get_service, get_spec and get_decision give the full text.
- To find what the specs say about something, use search_specs once, with the word and its alternatives: \
it returns only the requirements that mention them, best first, and when nothing matches the specs don't \
cover it. get_spec with outline lists a spec's requirements; read whole specs only when you need them.
- Issues are identified like FOR-12. Statuses, people, labels, projects and services are referred to by name.
- To clear a field, pass \"none\".
- An issue's description is its scope: build what it says. Comments are discussion. To change what an issue \
covers or how it's done, propose_description and let people decide; don't widen it in a comment.
- When you look for work, take your next issue in queue order: search_issues with assignee \"me\" lists \
your issues by priority, then age, and queue_position 1 is next. Don't skip ahead.
- When you pick up an issue, write its spec change first (save_spec_change), then call start_work, and \
finish_work when you're done with it, so the team sees what you're working on. When the work changes no \
code (only a spec or decision), pass no_code to finish_work: otherwise its reviewer expects a pull request. If start_work is refused \
because someone is already on it, leave the issue alone. If it's refused because of the spec change, write \
it, or write it again against the spec as it is now; when another issue changes the same requirement \
first, settle it with that issue's people (ask in Triage) instead of starting.
- Issues in Triage get a triage suggestion: search_issues with needs_triage lists those still waiting \
for one, and suggest_triage makes it for people to apply or dismiss.
- Before filing an issue, make sure it isn't there already. create_issue shows you open issues with \
similar titles first; file it only if none of them covers it.
- Specs change only when the work is done. Don't edit a spec: before you start an issue, write its spec \
change with save_spec_change (the requirements it adds, changes, removes or renames, and the decisions \
it replaces), or say why no spec changes. An issue can't be started without one. If you can't tell what \
the spec change is, the issue isn't clear enough: ask in Triage. Forge applies it when the issue is done.
- When you check the code for drift from the specs, report the check: start_drift_check with a key of \
your own, report_drift_check as verdicts come in, and finish_drift_check at the end, with usage when you \
know it. How you check is up to you.
- Everything you change is attributed to you and shows up live for the team. When you finish work on an \
issue, move it to the right status and say what you did in a comment.";

/// The routes for `/mcp`, behind the agent token check.
pub fn router(state: AppState) -> Router<AppState> {
    let pool = state.pool.clone();
    let mut config = StreamableHttpServerConfig::default();
    // Stateless: each request carries its token; nothing to keep between calls.
    config.legacy_session_mode = false;
    config.json_response = true;
    if let Ok(hosts) = std::env::var("FORGE_MCP_ALLOWED_HOSTS") {
        config.allowed_hosts.extend(
            hosts
                .split(',')
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty()),
        );
    }
    let service = StreamableHttpService::new(
        move || Ok(Forge::new(pool.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(
            state,
            agents::require_token,
        ))
}

#[derive(Clone)]
pub struct Forge {
    pool: PgPool,
    tool_router: ToolRouter<Self>,
}

/// The agent making the call, put on the request by `agents::require_token`.
fn actor(parts: &Parts) -> ToolResult<Actor> {
    parts
        .extensions
        .get::<Actor>()
        .copied()
        .ok_or_else(|| "not authenticated".to_string())
}

#[tool_router]
impl Forge {
    pub fn new(pool: PgPool) -> Self {
        Forge {
            pool,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "The workspace at a glance: teams and their statuses, members, labels, projects and services. Call this first."
    )]
    async fn workspace_overview(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        workspace_overview(&self.pool, actor(&parts)?).await
    }

    #[tool(
        description = "Find issues by words, team, status, assignee, project or service. Open issues only unless include_closed. With an assignee, the results come in queue order (next to take up first); issues waiting in an agent's queue carry their queue_position."
    )]
    async fn search_issues(
        &self,
        Parameters(input): Parameters<SearchIssues>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        search_issues(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "One issue in full: fields, description, comments, sub-issues, relations, recent history (who changed what), and the specs and decisions it follows: its service's, its project's and workspace-wide ones."
    )]
    async fn get_issue(
        &self,
        Parameters(input): Parameters<IssueRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        get_issue(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Create an issue in a team. When open issues have similar titles, nothing is created yet and you get them instead: if one covers it, use that one; if it's really new, call again with confirm_new: true."
    )]
    async fn create_issue(
        &self,
        Parameters(input): Parameters<CreateIssue>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        create_issue(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Change an issue's fields: title, description, status, priority, assignee, reviewer (the person asked to review it), labels, project, service, parent, due date."
    )]
    async fn update_issue(
        &self,
        Parameters(input): Parameters<UpdateIssue>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        update_issue(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Delete an issue you created, with its comments and relations (only admins delete others' issues). Prefer moving it to Canceled."
    )]
    async fn delete_issue(
        &self,
        Parameters(input): Parameters<IssueRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        delete_issue(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Comment on an issue (Markdown). Mention people as @Full Name to notify them."
    )]
    async fn comment_on_issue(
        &self,
        Parameters(input): Parameters<CommentOnIssue>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        comment_on_issue(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Propose a new description for an issue, for people to accept or dismiss on the issue. Use it instead of a comment when the issue's scope or approach should change: the description is what gets built."
    )]
    async fn propose_description(
        &self,
        Parameters(input): Parameters<ProposeDescription>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        propose_description(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Say you're starting work on an issue, so the team sees it. Call it when you pick up an issue, right after writing its spec change (save_spec_change) and before any other work. It's refused until the issue has its spec change, while its spec change is held back (another open issue changes the same requirement first, or it was written against text that has changed since; get_issue says which), while someone else is working on that issue, or while you're still working on another one (finish that first); when it's refused, don't work on the issue."
    )]
    async fn start_work(
        &self,
        Parameters(input): Parameters<IssueRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        start_work(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Say you've finished working on an issue for now: handed it over for review, asked for a decision, or stopped. Moving it to In Review or Done, or assigning it to someone else, also finishes it. Pass no_code when the work changes no code (only a spec or decision), so its reviewer can approve it without a pull request."
    )]
    async fn finish_work(
        &self,
        Parameters(input): Parameters<FinishWork>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        finish_work(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Suggest triage for an issue in Triage, the way Forge AI does: priority, labels to add, service, project, assignee (a person, or an agent such as you for a change to the code an agent can build), and duplicate or related issues. Leave out anything you aren't fairly confident about. People apply or dismiss it; nothing changes until they do. An issue has one waiting at a time. search_issues with needs_triage finds the issues that still need one."
    )]
    async fn suggest_triage(
        &self,
        Parameters(input): Parameters<SuggestTriage>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        suggest_triage(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "Relate two issues: blocks, blocked_by, related or duplicate_of.")]
    async fn relate_issues(
        &self,
        Parameters(input): Parameters<RelateIssues>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        relate_issues(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "All projects with status, lead and progress.")]
    async fn list_projects(&self, Extension(parts): Extension<Parts>) -> Result<String, String> {
        list_projects(&self.pool, actor(&parts)?).await
    }

    #[tool(description = "One project with its description, issues, specs and decisions.")]
    async fn get_project(
        &self,
        Parameters(input): Parameters<ProjectRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        get_project(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "Create a project (leave out `project`), or change one (give `project`).")]
    async fn save_project(
        &self,
        Parameters(input): Parameters<SaveProject>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        save_project(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "All services: what exists in the system and where its code lives.")]
    async fn list_services(&self, Extension(parts): Extension<Parts>) -> Result<String, String> {
        list_services(&self.pool, actor(&parts)?).await
    }

    #[tool(
        description = "One service in full: description, current state, specs and decisions with their text, open issues."
    )]
    async fn get_service(
        &self,
        Parameters(input): Parameters<ServiceRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        get_service(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Create a service (leave out `service`), or change one (give `service`). Update current_state when the code changes what the service does."
    )]
    async fn save_service(
        &self,
        Parameters(input): Parameters<SaveService>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        save_service(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "Specs, optionally for one service.")]
    async fn list_specs(
        &self,
        Parameters(input): Parameters<ListKnowledge>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        list_specs(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "The requirements that mention any of the words, those mentioning more of them first, each under its spec with the words it mentions; the best 10 in full with their scenarios, the next 30 by name. Optionally for one service. Specs without requirements that mention them are named. Search once with the word and its alternatives, e.g. `GitHub \"sign in\" login OAuth` (a quoted phrase is one word). When nothing matches, the specs don't mention it: don't search again with rephrasings."
    )]
    async fn search_specs(
        &self,
        Parameters(input): Parameters<SearchSpecs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        search_specs(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Start a drift check you run yourself, so the team can follow it in Forge: your own key for it, the commit range, the services it covers, the checker's name and version, whether it's a full audit, and optionally the requirements you plan to judge. Starting again with the same key answers with that check and changes nothing."
    )]
    async fn start_drift_check(
        &self,
        Parameters(input): Parameters<StartDriftCheck>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        start_drift_check(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Report verdicts into your running drift check: met, not_met, unclear, skipped, or covered (naming the open issue), each with why. A requirement reported again replaces its earlier verdict. If anything doesn't resolve, nothing is recorded and the answer says what to fix. Met, not met and unclear also become the requirements' latest verdicts on the spec pages."
    )]
    async fn report_drift_check(
        &self,
        Parameters(input): Parameters<ReportDriftCheck>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        report_drift_check(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "End your drift check as finished or failed, with a summary, the reason for a failed one, and usage per part (orchestrator, each worker) when you know it. Planned requirements without a verdict show the check as incomplete. The answer lists the issues Forge saw you file and comment on meanwhile."
    )]
    async fn finish_drift_check(
        &self,
        Parameters(input): Parameters<FinishDriftCheck>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        finish_drift_check(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "One spec in full, with each requirement's latest drift-check verdict (evidence, commit, and whether the requirement changed since), or with `outline` only the names of its requirements and their scenarios."
    )]
    async fn get_spec(
        &self,
        Parameters(input): Parameters<SpecRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        get_spec(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Create a spec (leave out `spec`), or change one (give `spec`). The body is Markdown and replaces the whole spec. A spec that describes how something behaves follows OpenSpec's convention as-is: `## Purpose`, then `## Requirements`; each requirement is `### Requirement: <name>`, a sentence with SHALL or MUST, and at least one `#### Scenario: <name>` with `- **WHEN** …` / `- **THEN** …` / `- **AND** …` bullets. A requirement's name is its identity, so rename one only on purpose. Overviews stay prose, and known gaps are issues, not requirements. Forge checks a spec with a `## Requirements` section the way `openspec validate` does and answers with what doesn't follow the convention; it's saved either way. Agents write an issue's spec change with save_spec_change instead, which Forge applies when the issue is done; they only use this to put in by hand a part of a done issue's spec change that no longer fitted its spec then (`applying`)."
    )]
    async fn save_spec(
        &self,
        Parameters(input): Parameters<SaveSpec>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        save_spec(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Write an issue's spec change: what it changes in a spec, the decisions it replaces, or why no spec changes. Write it before you start the issue (start_work and moving it to a started status need it); while someone else works on the issue it's theirs to write. Agents don't edit specs directly; the change takes effect when the issue is done. Give one spec per call (`spec`, or `new_spec` for a new one) with either `requirements`, in OpenSpec's delta format as-is (`## ADDED Requirements`, `## MODIFIED Requirements` and `## REMOVED Requirements` with `### Requirement: <name>` blocks, each added or modified one in full: the SHALL or MUST sentence and its scenarios; `## RENAMED Requirements` with `- FROM: `### Requirement: Old`` / `- TO: `### Requirement: New`` pairs), or `body`, the spec's whole new text (a rewrite, or a new spec). Writing a spec again replaces its part; `drop` takes it out. `replaces` and `no_spec_change` are for the whole issue. get_issue shows the spec change."
    )]
    async fn save_spec_change(
        &self,
        Parameters(input): Parameters<SaveSpecChange>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        save_spec_change(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "Decisions, optionally for one service.")]
    async fn list_decisions(
        &self,
        Parameters(input): Parameters<ListKnowledge>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        list_decisions(&self.pool, actor(&parts)?, input).await
    }

    #[tool(description = "One decision in full: context, decision, consequences, status.")]
    async fn get_decision(
        &self,
        Parameters(input): Parameters<DecisionRef>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        get_decision(&self.pool, actor(&parts)?, input).await
    }

    #[tool(
        description = "Record a decision (leave out `decision_ref`), or change one (give `decision_ref`). Mark old decisions superseded or overridden rather than deleting them."
    )]
    async fn save_decision(
        &self,
        Parameters(input): Parameters<SaveDecision>,
        Extension(parts): Extension<Parts>,
    ) -> Result<String, String> {
        save_decision(&self.pool, actor(&parts)?, input).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Forge {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("forge", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
