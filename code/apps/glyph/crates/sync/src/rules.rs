//! Glyph's rules for the shared sync engine ([`code_villain_sync`]): who may
//! change what, texts written by hand, and what a deleted language takes
//! with it.

use code_villain_sync::Rules;
use glyph_domain::{Actor, HUMAN, ModelKind, Operation};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Glyph, as the sync engine sees it.
pub struct Glyph;

impl Rules for Glyph {
    type Model = ModelKind;
    const NOTIFY_CHANNEL: &'static str = NOTIFY_CHANNEL;

    async fn check_allowed(
        db: &mut PgConnection,
        actor: Actor,
        operation: &Operation,
    ) -> Result<(), SyncError> {
        let (Operation::Create { model, .. }
        | Operation::Update { model, .. }
        | Operation::Delete { model, .. }) = operation;
        check_allowed(db, actor, *model).await
    }

    async fn prepare_update(
        _db: &mut PgConnection,
        _actor: Actor,
        model: ModelKind,
        _id: Uuid,
        _before: Option<&Value>,
        data: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        if model == ModelKind::Translation {
            written_by_hand(data);
        }
        Ok(())
    }

    async fn cascade(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        cascade(db, actor, model, id).await
    }

    fn constraint_words(constraint: &str) -> Option<&'static str> {
        plain_words(constraint)
    }
}

/// A new text that doesn't say who wrote it was written by a person, who is
/// sure of it: it stops counting as the model's.
fn written_by_hand(data: &mut Map<String, Value>) {
    if data.contains_key("value") && !data.contains_key("translated_by") {
        data.insert("translated_by".into(), json!(HUMAN));
        data.entry("confidence").or_insert(Value::Null);
    }
}

/// Who may make a write from a client. Members may write translations and
/// the glossary; the workspace's languages and how each pair is translated
/// are for admins.
async fn check_allowed(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
) -> Result<(), SyncError> {
    let refuse =
        |message: &str| -> Result<(), SyncError> { Err(SyncError::Forbidden(message.to_string())) };
    match model {
        ModelKind::Language if !is_admin(db, actor).await? => {
            refuse("Only admins can add or remove languages.")
        }
        ModelKind::LanguagePair if !is_admin(db, actor).await? => {
            refuse("Only admins can change how languages are translated.")
        }
        _ => Ok(()),
    }
}

/// Whether the actor is an admin of the workspace.
async fn is_admin(db: &mut PgConnection, actor: Actor) -> Result<bool, SyncError> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .fetch_optional(&mut *db)
    .await?;
    Ok(role.as_deref() == Some("admin"))
}

/// The operations that must happen before a record is deleted: deleting what
/// belongs to it.
async fn cascade(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    id: Uuid,
) -> Result<Vec<Operation>, SyncError> {
    let ids = async |db: &mut PgConnection, sql: &'static str| -> Result<Vec<Uuid>, SyncError> {
        Ok(sqlx::query_scalar(sql)
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_all(&mut *db)
            .await?)
    };
    let delete = |model: ModelKind| move |id: Uuid| Operation::Delete { model, id };

    let mut operations = Vec::new();
    match model {
        // A language takes its texts, suggestions, glossary, pair settings
        // and runs with it.
        ModelKind::Language => {
            let translations = ids(
                db,
                "SELECT t.id FROM translations t JOIN languages l \
                 ON l.workspace_id = t.workspace_id AND l.code = t.lang \
                 WHERE l.id = $1 AND l.workspace_id = $2",
            )
            .await?;
            operations.extend(translations.into_iter().map(delete(ModelKind::Translation)));
            let suggestions = ids(
                db,
                "SELECT s.id FROM suggestions s JOIN languages l \
                 ON l.workspace_id = s.workspace_id AND l.code = s.lang \
                 WHERE l.id = $1 AND l.workspace_id = $2",
            )
            .await?;
            operations.extend(suggestions.into_iter().map(delete(ModelKind::Suggestion)));
            let entries = ids(
                db,
                "SELECT g.id FROM glossary_entries g JOIN languages l \
                 ON l.workspace_id = g.workspace_id AND l.code IN (g.source_lang, g.target_lang) \
                 WHERE l.id = $1 AND l.workspace_id = $2",
            )
            .await?;
            operations.extend(entries.into_iter().map(delete(ModelKind::GlossaryEntry)));
            let pairs = ids(
                db,
                "SELECT p.id FROM language_pairs p JOIN languages l \
                 ON l.workspace_id = p.workspace_id AND l.code IN (p.source_lang, p.target_lang) \
                 WHERE l.id = $1 AND l.workspace_id = $2",
            )
            .await?;
            operations.extend(pairs.into_iter().map(delete(ModelKind::LanguagePair)));
            let runs = ids(
                db,
                "SELECT r.id FROM translation_runs r JOIN languages l \
                 ON l.workspace_id = r.workspace_id AND l.code IN (r.source_lang, r.target_lang) \
                 WHERE l.id = $1 AND l.workspace_id = $2",
            )
            .await?;
            operations.extend(runs.into_iter().map(delete(ModelKind::TranslationRun)));
        }
        ModelKind::Translation
        | ModelKind::GlossaryEntry
        | ModelKind::LanguagePair
        | ModelKind::Suggestion
        | ModelKind::TranslationRun => {}
        ModelKind::User => {
            return Err(SyncError::Invalid(format!(
                "{} records can't be deleted this way",
                model.name()
            )));
        }
    }
    Ok(operations)
}

/// What a broken constraint means, for the ones people run into.
fn plain_words(constraint: &str) -> Option<&'static str> {
    Some(match constraint {
        "languages_workspace_id_code_key" => "That language is already in the workspace.",
        "languages_one_source_idx" => "Only one language can be the source.",
        "translations_workspace_id_lang_namespace_key_key" => {
            "That key already exists in this namespace."
        }
        "glossary_entries_term_idx" => "There's already a glossary entry for that term.",
        "language_pairs_workspace_id_source_lang_target_lang_key" => {
            "Those two languages already have settings."
        }
        c if c.ends_with("_lang_fkey") => "Add that language to the workspace first.",
        _ => return None,
    })
}
