//! Translating with the models: runs, which a worker carries out key by
//! key, and asking a model for a glossary term's forms.
//!
//! A run's progress and results are synced records like any other: the
//! worker writes them through the sync engine, so every client sees
//! suggestions arrive and the count go up.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use glyph_domain::{
    Actor, DEFAULT_NAMESPACE, ModelKind, Operation, RunKind, RunStatus, StartRunRequest,
    Transaction, TranslationRun, Validate, Variants, VariantsRequest,
};
use glyph_llm::{Client, GlossaryTerm, LlmError, Model, Translated, languages};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// Finished runs are kept this long, then cleared when a new one starts.
const KEEP_RUNS: Duration = Duration::from_secs(7 * 24 * 60 * 60);

const NO_MODEL: &str =
    "No translation model is set up on this server. Add one to its models file (models.toml).";

/// A key to translate: its text in the source language.
#[derive(Debug, Clone, sqlx::FromRow)]
struct Item {
    namespace: String,
    key: String,
    value: String,
    context: String,
}

/// How a pair is translated: its model and temperature.
struct Settings {
    model: Model,
    temperature: f64,
}

/// `POST /api/w/{id}/runs`: starts translating, and answers with the run
/// at once. Its progress arrives through sync.
pub async fn start(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<StartRunRequest>,
) -> Result<(StatusCode, Json<TranslationRun>), AppError> {
    request.validate()?;
    let actor = member.actor;
    let settings = settings(&state, actor, &request.source_lang, &request.target_lang).await?;
    let items = plan(&state.pool, actor, &request).await?;
    if items.is_empty() {
        return Err(AppError::invalid(
            "Nothing to translate: every key already has a text or a suggestion.",
        ));
    }

    let id = Uuid::now_v7();
    let mut db = state.pool.begin().await?;
    let mut operations = old_runs(&mut db, actor).await?;
    operations.push(Operation::Create {
        model: ModelKind::TranslationRun,
        id,
        data: object(json!({
            "kind": request.kind,
            "source_lang": request.source_lang,
            "target_lang": request.target_lang,
            "namespace": request.namespace,
            "total": items.len(),
            "model": settings.model.name,
            "started_by": actor.user_id,
        })),
    });
    apply(&mut db, actor, operations).await?;
    let run = load_run(&mut db, actor, id)
        .await?
        .ok_or_else(|| AppError::internal("run vanished"))?;
    db.commit().await?;

    let named = request.keys.is_some();
    tokio::spawn(work(state, actor, run.clone(), items, settings, named));
    Ok((StatusCode::ACCEPTED, Json(run)))
}

/// `POST /api/w/{id}/runs/{run_id}/cancel`: stops a run after the key it's
/// on. A finished run stays as it is.
pub async fn cancel(
    State(state): State<AppState>,
    member: Member,
    Path((_, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TranslationRun>, AppError> {
    let actor = member.actor;
    let mut db = state.pool.begin().await?;
    glyph_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let run = load_run(&mut db, actor, run_id)
        .await?
        .ok_or_else(|| AppError::not_found("run not found"))?;
    if run.status.finished() {
        return Ok(Json(run));
    }
    apply(
        &mut db,
        actor,
        vec![patch_run(run_id, json!({ "status": RunStatus::Cancelled }))],
    )
    .await?;
    let run = load_run(&mut db, actor, run_id)
        .await?
        .ok_or_else(|| AppError::internal("run vanished"))?;
    db.commit().await?;
    Ok(Json(run))
}

/// `POST /api/w/{id}/glossary/variants`: the grammatical forms of a term,
/// from the pair's model.
pub async fn variants(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<VariantsRequest>,
) -> Result<Json<Variants>, AppError> {
    request.validate()?;
    let settings = settings(
        &state,
        member.actor,
        &request.source_lang,
        &request.target_lang,
    )
    .await?;
    let variants = Client::new(&settings.model)
        .variants(
            &languages::name(&request.source_lang),
            &languages::name(&request.target_lang),
            &request.source_term,
            &request.target_term,
        )
        .await
        .map_err(model_failed)?;
    Ok(Json(Variants { variants }))
}

fn model_failed(e: LlmError) -> AppError {
    tracing::warn!(error = %e, "the model failed");
    match e {
        LlmError::Unreachable { .. } => AppError::unavailable(
            "The model isn't answering. Start it in Settings, or check its endpoint.",
        ),
        other => AppError::unavailable(format!("The model failed: {other}")),
    }
}

/// Marks runs that were going when the server stopped as failed: nothing is
/// working on them any more.
pub async fn fail_interrupted(pool: &PgPool) -> Result<(), AppError> {
    let runs: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, workspace_id, started_by FROM translation_runs \
         WHERE status IN ('queued', 'running')",
    )
    .fetch_all(pool)
    .await?;
    for (id, workspace_id, user_id) in runs {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        let mut db = pool.begin().await?;
        let patch = patch_run(
            id,
            json!({ "status": RunStatus::Failed, "error": "The server restarted before the run finished." }),
        );
        apply(&mut db, actor, vec![patch]).await?;
        db.commit().await?;
    }
    Ok(())
}

/// The pair's model and temperature: its settings if it has some, otherwise
/// the first model, at 0.2.
async fn settings(
    state: &AppState,
    actor: Actor,
    source_lang: &str,
    target_lang: &str,
) -> Result<Settings, AppError> {
    let pair: Option<(String, f64)> = sqlx::query_as(
        "SELECT model, temperature FROM language_pairs \
         WHERE workspace_id = $1 AND source_lang = $2 AND target_lang = $3",
    )
    .bind(actor.workspace_id)
    .bind(source_lang)
    .bind(target_lang)
    .fetch_optional(&state.pool)
    .await?;
    let (name, temperature) = pair.unwrap_or_else(|| (String::new(), 0.2));
    let model = state
        .models
        .pick(&name)
        .cloned()
        .ok_or_else(|| AppError::invalid(NO_MODEL))?;
    Ok(Settings { model, temperature })
}

/// The keys a run translates, in order.
async fn plan(
    pool: &PgPool,
    actor: Actor,
    request: &StartRunRequest,
) -> Result<Vec<Item>, AppError> {
    let (namespaces, keys): (Option<Vec<String>>, Option<Vec<String>>) = match &request.keys {
        Some(refs) => (
            Some(refs.iter().map(|k| k.namespace.clone()).collect()),
            Some(refs.iter().map(|k| k.key.clone()).collect()),
        ),
        None => (None, None),
    };
    // Named keys are translated whether they're missing or not; otherwise
    // only missing ones, and for a review only ones with nothing waiting.
    Ok(sqlx::query_as(
        "SELECT s.namespace, s.key, s.value, s.context FROM translations s \
         WHERE s.workspace_id = $1 AND s.lang = $2 AND btrim(s.value) <> '' \
           AND ($4::text IS NULL OR s.namespace = $4) \
           AND ($5::text[] IS NULL OR (s.namespace, s.key) IN \
                (SELECT n, k FROM unnest($5::text[], $6::text[]) AS u (n, k))) \
           AND ($5::text[] IS NOT NULL OR NOT EXISTS ( \
                SELECT 1 FROM translations t WHERE t.workspace_id = s.workspace_id \
                AND t.lang = $3 AND t.namespace = s.namespace AND t.key = s.key \
                AND btrim(t.value) <> '')) \
           AND ($5::text[] IS NOT NULL OR NOT $7 OR NOT EXISTS ( \
                SELECT 1 FROM suggestions g WHERE g.workspace_id = s.workspace_id \
                AND g.lang = $3 AND g.namespace = s.namespace AND g.key = s.key)) \
         ORDER BY s.namespace = $8 DESC, s.namespace, s.key \
         LIMIT $9",
    )
    .bind(actor.workspace_id)
    .bind(&request.source_lang)
    .bind(&request.target_lang)
    .bind(&request.namespace)
    .bind(namespaces)
    .bind(keys)
    .bind(request.kind == RunKind::Suggest)
    .bind(DEFAULT_NAMESPACE)
    .bind(i64::from(request.limit()))
    .fetch_all(pool)
    .await?)
}

/// Deletes for the workspace's runs that finished long ago.
async fn old_runs(db: &mut PgConnection, actor: Actor) -> Result<Vec<Operation>, AppError> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM translation_runs WHERE workspace_id = $1 \
         AND status IN ('done', 'failed', 'cancelled') AND updated_at < now() - $2::interval",
    )
    .bind(actor.workspace_id)
    .bind(format!("{} seconds", KEEP_RUNS.as_secs()))
    .fetch_all(&mut *db)
    .await?;
    Ok(ids
        .into_iter()
        .map(|id| Operation::Delete {
            model: ModelKind::TranslationRun,
            id,
        })
        .collect())
}

/// Carries out a run: one key at a time, each written as soon as the model
/// answers. Runs take turns, since one model serves them all.
async fn work(
    state: AppState,
    actor: Actor,
    run: TranslationRun,
    items: Vec<Item>,
    settings: Settings,
    named: bool,
) {
    let _turn = state.llm_turn.acquire().await;
    let id = run.id;
    if let Err(e) = carry_out(&state, actor, &run, &items, &settings, named).await {
        tracing::error!(error = ?e, run = %id, "a translation run broke off");
        let _ = finish(
            &state.pool,
            actor,
            id,
            RunStatus::Failed,
            "Something went wrong on the server.",
        )
        .await;
    }
}

async fn carry_out(
    state: &AppState,
    actor: Actor,
    run: &TranslationRun,
    items: &[Item],
    settings: &Settings,
    named: bool,
) -> Result<(), AppError> {
    let pool = &state.pool;
    // Cancelled while it waited its turn.
    if still(pool, actor, run.id).await? != Some(RunStatus::Queued)
        || !update_run(pool, actor, run.id, json!({ "status": RunStatus::Running })).await?
    {
        return Ok(());
    }
    let glossary = glossary(pool, actor, &run.source_lang, &run.target_lang).await?;
    let client = Client::new(&settings.model);
    let (source, target) = (
        languages::name(&run.source_lang),
        languages::name(&run.target_lang),
    );
    let (mut done, mut failed) = (0, 0);

    for item in items {
        if still(pool, actor, run.id).await? != Some(RunStatus::Running) {
            return Ok(());
        }
        let request = glyph_llm::Request {
            source_lang: &source,
            target_lang: &target,
            text: &item.value,
            glossary: &glossary,
            context: &item.context,
            temperature: settings.temperature,
        };
        match client.translate(&request).await {
            Ok(translated) => {
                done += 1;
                let progress = patch_run(run.id, json!({ "done": done }));
                let written = match run.kind {
                    RunKind::Suggest => {
                        suggest(pool, actor, run, item, &translated, progress).await
                    }
                    RunKind::Fill => {
                        fill(pool, actor, run, item, &translated, progress, named).await
                    }
                };
                if let Err(e) = written {
                    if still(pool, actor, run.id).await?.is_none() {
                        return Ok(()); // The run went with its language.
                    }
                    return Err(e);
                }
            }
            Err(LlmError::Unreachable { endpoint, .. }) => {
                let error = format!(
                    "Couldn't reach the model at {endpoint}. Start it in Settings, or check its endpoint."
                );
                finish(pool, actor, run.id, RunStatus::Failed, &error).await?;
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(error = %e, key = item.key, "the model failed on a key");
                failed += 1;
                if !update_run(pool, actor, run.id, json!({ "failed": failed })).await? {
                    return Ok(());
                }
            }
        }
    }
    finish(pool, actor, run.id, RunStatus::Done, "").await
}

/// Writes a suggestion, replacing one already waiting for the key.
async fn suggest(
    pool: &PgPool,
    actor: Actor,
    run: &TranslationRun,
    item: &Item,
    translated: &Translated,
    progress: Operation,
) -> Result<(), AppError> {
    let mut db = pool.begin().await?;
    glyph_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let waiting: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM suggestions WHERE workspace_id = $1 AND lang = $2 AND namespace = $3 AND key = $4",
    )
    .bind(actor.workspace_id)
    .bind(&run.target_lang)
    .bind(&item.namespace)
    .bind(&item.key)
    .fetch_optional(&mut *db)
    .await?;
    let fields = json!({
        "run_id": run.id,
        "value": translated.text,
        "confidence": translated.confidence,
        "model": run.model,
        "source_text": item.value,
        "glossary_violations": translated.violations,
        "placeholders_broken": translated.placeholders_broken,
    });
    let write = match waiting {
        Some(id) => Operation::Update {
            model: ModelKind::Suggestion,
            id,
            data: object(fields),
        },
        None => {
            let mut data = object(fields);
            data.insert("lang".into(), json!(run.target_lang));
            data.insert("namespace".into(), json!(item.namespace));
            data.insert("key".into(), json!(item.key));
            Operation::Create {
                model: ModelKind::Suggestion,
                id: Uuid::now_v7(),
                data,
            }
        }
    };
    apply(&mut db, actor, vec![write, progress]).await?;
    db.commit().await?;
    Ok(())
}

/// Writes the model's text as the translation. A text someone wrote while
/// the run was going stays, unless the run named the key. A suggestion
/// waiting for the key goes: this replaces it.
async fn fill(
    pool: &PgPool,
    actor: Actor,
    run: &TranslationRun,
    item: &Item,
    translated: &Translated,
    progress: Operation,
    named: bool,
) -> Result<(), AppError> {
    let mut db = pool.begin().await?;
    glyph_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, value FROM translations WHERE workspace_id = $1 AND lang = $2 AND namespace = $3 AND key = $4",
    )
    .bind(actor.workspace_id)
    .bind(&run.target_lang)
    .bind(&item.namespace)
    .bind(&item.key)
    .fetch_optional(&mut *db)
    .await?;
    let waiting: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM suggestions WHERE workspace_id = $1 AND lang = $2 AND namespace = $3 AND key = $4",
    )
    .bind(actor.workspace_id)
    .bind(&run.target_lang)
    .bind(&item.namespace)
    .bind(&item.key)
    .fetch_all(&mut *db)
    .await?;

    let fields = json!({
        "value": translated.text,
        "confidence": translated.confidence,
        "translated_by": run.model,
    });
    let mut operations = Vec::new();
    match existing {
        Some((_, value)) if !named && !value.trim().is_empty() => {}
        Some((id, _)) => operations.push(Operation::Update {
            model: ModelKind::Translation,
            id,
            data: object(fields),
        }),
        None => {
            let mut data = object(fields);
            data.insert("lang".into(), json!(run.target_lang));
            data.insert("namespace".into(), json!(item.namespace));
            data.insert("key".into(), json!(item.key));
            operations.push(Operation::Create {
                model: ModelKind::Translation,
                id: Uuid::now_v7(),
                data,
            });
        }
    }
    if !operations.is_empty() {
        operations.extend(waiting.into_iter().map(|id| Operation::Delete {
            model: ModelKind::Suggestion,
            id,
        }));
    }
    operations.push(progress);
    apply(&mut db, actor, operations).await?;
    db.commit().await?;
    Ok(())
}

/// The pair's glossary.
async fn glossary(
    pool: &PgPool,
    actor: Actor,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<GlossaryTerm>, AppError> {
    let rows: Vec<(String, String, Vec<String>)> = sqlx::query_as(
        "SELECT source_term, target_term, target_variants FROM glossary_entries \
         WHERE workspace_id = $1 AND source_lang = $2 AND target_lang = $3 ORDER BY source_term",
    )
    .bind(actor.workspace_id)
    .bind(source_lang)
    .bind(target_lang)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(source_term, target_term, target_variants)| GlossaryTerm {
            source_term,
            target_term,
            target_variants,
        })
        .collect())
}

/// The run's status now, or `None` once it's gone.
async fn still(pool: &PgPool, actor: Actor, id: Uuid) -> Result<Option<RunStatus>, AppError> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM translation_runs WHERE id = $1 AND workspace_id = $2",
    )
    .bind(id)
    .bind(actor.workspace_id)
    .fetch_optional(pool)
    .await?;
    Ok(status.and_then(|s| serde_json::from_value(Value::String(s)).ok()))
}

/// Ends a run, unless someone cancelled it first.
async fn finish(
    pool: &PgPool,
    actor: Actor,
    id: Uuid,
    status: RunStatus,
    error: &str,
) -> Result<(), AppError> {
    if still(pool, actor, id)
        .await?
        .is_some_and(RunStatus::finished)
    {
        return Ok(());
    }
    update_run(pool, actor, id, json!({ "status": status, "error": error })).await?;
    Ok(())
}

/// Changes a run in a transaction of its own; `false` when it's gone.
async fn update_run(
    pool: &PgPool,
    actor: Actor,
    id: Uuid,
    fields: Value,
) -> Result<bool, AppError> {
    let mut db = pool.begin().await?;
    match apply(&mut db, actor, vec![patch_run(id, fields)]).await {
        Ok(()) => {}
        Err(e) if e.is_not_found() => return Ok(false),
        Err(e) => return Err(e),
    }
    db.commit().await?;
    Ok(true)
}

fn patch_run(id: Uuid, fields: Value) -> Operation {
    Operation::Update {
        model: ModelKind::TranslationRun,
        id,
        data: object(fields),
    }
}

async fn load_run(
    db: &mut PgConnection,
    actor: Actor,
    id: Uuid,
) -> Result<Option<TranslationRun>, AppError> {
    let row: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(r.*) FROM translation_runs r WHERE r.id = $1 AND r.workspace_id = $2",
    )
    .bind(id)
    .bind(actor.workspace_id)
    .fetch_optional(&mut *db)
    .await?;
    row.map(serde_json::from_value)
        .transpose()
        .map_err(AppError::internal)
}

async fn apply(
    db: &mut PgConnection,
    actor: Actor,
    operations: Vec<Operation>,
) -> Result<(), AppError> {
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    glyph_sync::apply_in(db, actor, &transaction, glyph_sync::Origin::Server).await?;
    Ok(())
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
