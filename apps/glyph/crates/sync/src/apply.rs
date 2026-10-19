use glyph_domain::{Actor, HUMAN, ModelKind, Operation, SyncAction, Transaction, Write};
use serde_json::{Map, Value, json};
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Who a transaction comes from. Client writes (the app, the API) follow the
/// rules about who may change what (`check_allowed`); the server's own
/// writes, such as cascades, imports and the translation worker's, don't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Client,
    Server,
}

/// Applies a client's transaction atomically and returns the sync id of its
/// last change, or `None` if it changed nothing.
///
/// Applying the same transaction id again returns the first result without
/// changing anything, so clients can retry safely.
pub async fn apply_transaction(
    pool: &PgPool,
    actor: Actor,
    transaction: &Transaction,
) -> Result<Option<i64>, SyncError> {
    let mut db = pool.begin().await?;
    let result = apply_in(&mut db, actor, transaction, Origin::Client).await?;
    db.commit().await?;
    Ok(result)
}

/// Applies a transaction inside the caller's database transaction, for
/// server code that changes synced records alongside other work.
pub async fn apply_in(
    db: &mut PgConnection,
    actor: Actor,
    transaction: &Transaction,
    origin: Origin,
) -> Result<Option<i64>, SyncError> {
    lock_workspace(db, actor.workspace_id).await?;

    let applied: Option<(Uuid, Option<i64>)> =
        sqlx::query_as("SELECT workspace_id, last_sync_id FROM sync_transactions WHERE id = $1")
            .bind(transaction.id)
            .fetch_optional(&mut *db)
            .await?;
    if let Some((workspace_id, last_sync_id)) = applied {
        if workspace_id != actor.workspace_id {
            return Err(SyncError::Conflict("transaction id already used".into()));
        }
        return Ok(last_sync_id);
    }

    let mut last_sync_id = None;
    for operation in &transaction.operations {
        if let Some(sync_id) = apply_operation(db, actor, transaction.id, operation, origin).await?
        {
            last_sync_id = Some(sync_id);
        }
    }

    sqlx::query(
        "INSERT INTO sync_transactions (id, workspace_id, actor_id, last_sync_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(transaction.id)
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .bind(last_sync_id)
    .execute(&mut *db)
    .await?;

    if last_sync_id.is_some() {
        notify(db, actor.workspace_id).await?;
    }
    Ok(last_sync_id)
}

/// Records a change the server made to a record outside of a transaction,
/// such as someone joining the workspace (a `User`). Clients hear about it
/// like any other change.
pub async fn record_change(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    model_id: Uuid,
    action: SyncAction,
    data: Option<Value>,
) -> Result<i64, SyncError> {
    lock_workspace(db, actor.workspace_id).await?;
    let sync_id = log(
        db,
        actor,
        Uuid::now_v7(),
        model,
        model_id,
        action,
        data,
        None,
    )
    .await?;
    notify(db, actor.workspace_id).await?;
    Ok(sync_id)
}

/// Writes to a workspace take turns. That keeps its sync ids in commit
/// order, so a client that has seen sync id N has seen everything before it.
pub async fn lock_workspace(db: &mut PgConnection, workspace_id: Uuid) -> Result<(), SyncError> {
    let locked = sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace_id)
        .fetch_optional(&mut *db)
        .await?;
    if locked.is_none() {
        return Err(SyncError::NotFound("workspace".into()));
    }
    Ok(())
}

/// Delivered when the database transaction commits.
async fn notify(db: &mut PgConnection, workspace_id: Uuid) -> Result<(), SyncError> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(NOTIFY_CHANNEL)
        .bind(workspace_id.to_string())
        .execute(&mut *db)
        .await?;
    Ok(())
}

async fn apply_operation(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    operation: &Operation,
    origin: Origin,
) -> Result<Option<i64>, SyncError> {
    let (model, write) = match operation {
        Operation::Create { model, .. } => (*model, Write::Create),
        Operation::Update { model, .. } => (*model, Write::Update),
        Operation::Delete { model, .. } => (*model, Write::Delete),
    };
    if origin == Origin::Client {
        if !model.client_may(write) {
            return Err(SyncError::Invalid(format!(
                "{} records can't be changed this way",
                model.name()
            )));
        }
        check_allowed(db, actor, model).await?;
    }

    match operation {
        Operation::Create { model, id, data } => {
            model.validate_create(data)?;
            let mut row = data.clone();
            row.insert("id".into(), json!(id));
            row.insert("workspace_id".into(), json!(actor.workspace_id));

            let record = insert(db, *model, row).await?;
            let visible_to = owner(*model, &record);
            let sync_id = log(
                db,
                actor,
                transaction_id,
                *model,
                *id,
                SyncAction::Insert,
                Some(record),
                visible_to,
            )
            .await?;
            Ok(Some(sync_id))
        }
        Operation::Update { model, id, data } => {
            model.validate_patch(data)?;
            if data.is_empty() {
                return Ok(None);
            }
            let mut data = data.clone();
            if *model == ModelKind::Translation {
                written_by_hand(&mut data);
            }
            let record = update(db, actor, *model, *id, &data)
                .await?
                .ok_or_else(|| SyncError::NotFound(model.name().into()))?;
            let visible_to = owner(*model, &record);
            let sync_id = log(
                db,
                actor,
                transaction_id,
                *model,
                *id,
                SyncAction::Update,
                Some(record),
                visible_to,
            )
            .await?;
            Ok(Some(sync_id))
        }
        Operation::Delete { model, id } => {
            // Delete what depends on the record first, through the log, so
            // clients' copies stay exact.
            let mut last = None;
            for dependent in cascade(db, actor, *model, *id).await? {
                if let Some(sync_id) = Box::pin(apply_operation(
                    db,
                    actor,
                    transaction_id,
                    &dependent,
                    Origin::Server,
                ))
                .await?
                {
                    last = Some(sync_id);
                }
            }

            let Some(record) = current(db, actor, *model, *id).await? else {
                return Ok(last);
            };
            let sql = format!(
                "DELETE FROM {} WHERE id = $1 AND workspace_id = $2",
                model.table()
            );
            // Safe: the table name is a constant from `ModelKind`.
            sqlx::query(AssertSqlSafe(sql))
                .bind(id)
                .bind(actor.workspace_id)
                .execute(&mut *db)
                .await?;
            let visible_to = owner(*model, &record);
            let sync_id = log(
                db,
                actor,
                transaction_id,
                *model,
                *id,
                SyncAction::Delete,
                None,
                visible_to,
            )
            .await?;
            Ok(Some(sync_id))
        }
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

/// A record as it is now, or `None` if it doesn't exist.
async fn current(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    id: Uuid,
) -> Result<Option<Value>, SyncError> {
    let sql = format!(
        "SELECT to_jsonb(t.*) FROM {} AS t WHERE t.id = $1 AND t.workspace_id = $2",
        model.table()
    );
    // Safe: the table name is a constant from `ModelKind`.
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_optional(&mut *db)
        .await?)
}

/// For records only one person syncs, that person.
fn owner(model: ModelKind, record: &Value) -> Option<Uuid> {
    if !model.private_to_user() {
        return None;
    }
    record
        .get("user_id")
        .and_then(Value::as_str)
        .and_then(|id| id.parse().ok())
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
        // A language takes its texts, glossary and pair settings with it.
        ModelKind::Language => {
            let translations = ids(
                db,
                "SELECT t.id FROM translations t JOIN languages l \
                 ON l.workspace_id = t.workspace_id AND l.code = t.lang \
                 WHERE l.id = $1 AND l.workspace_id = $2",
            )
            .await?;
            operations.extend(translations.into_iter().map(delete(ModelKind::Translation)));
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
        }
        ModelKind::Translation | ModelKind::GlossaryEntry | ModelKind::LanguagePair => {}
        ModelKind::User => {
            return Err(SyncError::Invalid(format!(
                "{} records can't be deleted this way",
                model.name()
            )));
        }
    }
    Ok(operations)
}

/// Inserts `row` into the model's table, letting Postgres convert the JSON
/// values to column types. Columns not in `row` get their defaults.
async fn insert(
    db: &mut PgConnection,
    model: ModelKind,
    row: Map<String, Value>,
) -> Result<Value, SyncError> {
    let table = model.table();
    let columns = column_list(&row)?;
    let sql = format!(
        "INSERT INTO {table} ({columns}) SELECT {columns} FROM jsonb_populate_record(NULL::{table}, $1) \
         RETURNING to_jsonb({table}.*)"
    );
    // Safe: the table name is a constant and `column_list` checked the columns.
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(Value::Object(row))
        .fetch_one(&mut *db)
        .await?)
}

/// Sets the fields in `data` on one record and returns the whole record, or
/// `None` if it doesn't exist.
async fn update(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    id: Uuid,
    data: &Map<String, Value>,
) -> Result<Option<Value>, SyncError> {
    let table = model.table();
    column_list(data)?;
    let assignments: Vec<String> = data.keys().map(|c| format!("{c} = r.{c}")).collect();
    let sql = format!(
        "UPDATE {table} AS t SET {}, updated_at = now() \
         FROM jsonb_populate_record(NULL::{table}, $1) AS r \
         WHERE t.id = $2 AND t.workspace_id = $3 RETURNING to_jsonb(t.*)",
        assignments.join(", ")
    );
    // Safe: the table name is a constant and `column_list` checked the columns.
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(Value::Object(data.clone()))
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_optional(&mut *db)
        .await?)
}

/// The keys of `row` as a column list. Validation has already rejected
/// unknown fields; this check keeps arbitrary text out of the SQL regardless.
fn column_list(row: &Map<String, Value>) -> Result<String, SyncError> {
    let valid = |c: &str| {
        c.starts_with(|ch: char| ch.is_ascii_lowercase())
            && c.chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    };
    if let Some(bad) = row.keys().find(|c| !valid(c)) {
        return Err(SyncError::Invalid(format!("invalid field name {bad:?}")));
    }
    Ok(row
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", "))
}

#[allow(clippy::too_many_arguments)]
async fn log(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    model: ModelKind,
    model_id: Uuid,
    action: SyncAction,
    data: Option<Value>,
    visible_to: Option<Uuid>,
) -> Result<i64, SyncError> {
    let action = match action {
        SyncAction::Insert => "insert",
        SyncAction::Update => "update",
        SyncAction::Delete => "delete",
    };
    Ok(sqlx::query_scalar(
        "INSERT INTO sync_log (workspace_id, transaction_id, actor_id, model, model_id, action, data, visible_to) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING sync_id",
    )
    .bind(actor.workspace_id)
    .bind(transaction_id)
    .bind(actor.user_id)
    .bind(model.name())
    .bind(model_id)
    .bind(action)
    .bind(data)
    .bind(visible_to)
    .fetch_one(&mut *db)
    .await?)
}
