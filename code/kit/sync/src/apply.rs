use std::future::Future;
use std::pin::Pin;

use serde_json::{Map, Value, json};
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

use crate::{Actor, Model, Operation, SyncAction, SyncError, Transaction, Write};

/// Who a transaction comes from. Client writes (the app, agents, the API)
/// follow the app's rules about who may change what
/// ([`Rules::check_allowed`]); the server's own writes, such as cascades and
/// the effects of a change, don't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Client,
    Server,
}

/// What an app adds to the engine: its models, who may change what, and what
/// else a change does. The hooks that aren't needed can be left out.
pub trait Rules: Send + Sync + 'static {
    type Model: Model;

    /// The Postgres channel notified, with the workspace id as payload,
    /// whenever a workspace's log grows.
    const NOTIFY_CHANNEL: &'static str;

    /// Who may make a client's write, once [`Model::client_may`] allowed
    /// that kind of write.
    fn check_allowed(
        _db: &mut PgConnection,
        _actor: Actor,
        _operation: &Operation<Self::Model>,
    ) -> impl Future<Output = Result<(), SyncError>> + Send {
        async { Ok(()) }
    }

    /// Fills in fields only the server may set when a record is created.
    fn prepare_create(
        _db: &mut PgConnection,
        _actor: Actor,
        _model: Self::Model,
        _row: &mut Map<String, Value>,
    ) -> impl Future<Output = Result<(), SyncError>> + Send {
        async { Ok(()) }
    }

    /// Fills in fields that follow from an update, given the record before it.
    fn prepare_update(
        _db: &mut PgConnection,
        _actor: Actor,
        _model: Self::Model,
        _id: Uuid,
        _before: Option<&Value>,
        _data: &mut Map<String, Value>,
    ) -> impl Future<Output = Result<(), SyncError>> + Send {
        async { Ok(()) }
    }

    /// Rules checked after a create or update, which the database's
    /// constraints can't express.
    fn check_rules(
        _db: &mut PgConnection,
        _actor: Actor,
        _model: Self::Model,
        _id: Uuid,
    ) -> impl Future<Output = Result<(), SyncError>> + Send {
        async { Ok(()) }
    }

    /// What else a create (`before` is `None`) or update does, such as
    /// notifying people, in the same transaction. Returns the sync id of the
    /// last change it logged; changes to synced records go through
    /// [`apply_operation`] with [`Origin::Server`], or [`insert`] and [`log`].
    fn after_write(
        _db: &mut PgConnection,
        _actor: Actor,
        _transaction_id: Uuid,
        _model: Self::Model,
        _before: Option<&Value>,
        _after: &Value,
    ) -> impl Future<Output = Result<Option<i64>, SyncError>> + Send {
        async { Ok(None) }
    }

    /// The operations that must happen before a record is deleted: deleting
    /// what belongs to it and clearing references to it.
    fn cascade(
        _db: &mut PgConnection,
        _actor: Actor,
        _model: Self::Model,
        _id: Uuid,
    ) -> impl Future<Output = Result<Vec<Operation<Self::Model>>, SyncError>> + Send {
        async { Ok(Vec::new()) }
    }

    /// Runs just before an existing record is deleted, after its cascade.
    fn before_delete(
        _db: &mut PgConnection,
        _actor: Actor,
        _transaction_id: Uuid,
        _model: Self::Model,
        _id: Uuid,
    ) -> impl Future<Output = Result<(), SyncError>> + Send {
        async { Ok(()) }
    }

    /// What a broken constraint means, in plain words, for the ones people
    /// run into. Others keep the database's message.
    fn constraint_words(_constraint: &str) -> Option<&'static str> {
        None
    }
}

/// Applies a client's transaction atomically and returns the sync id of its
/// last change, or `None` if it changed nothing.
///
/// Applying the same transaction id again returns the first result without
/// changing anything, so clients can retry safely.
pub async fn apply_transaction<R: Rules>(
    pool: &PgPool,
    actor: Actor,
    transaction: &Transaction<R::Model>,
) -> Result<Option<i64>, SyncError> {
    let mut db = pool.begin().await?;
    let result = apply_in::<R>(&mut db, actor, transaction, Origin::Client).await?;
    db.commit().await?;
    Ok(result)
}

/// Applies a transaction inside the caller's database transaction, for
/// server code that changes synced records alongside other work.
pub async fn apply_in<R: Rules>(
    db: &mut PgConnection,
    actor: Actor,
    transaction: &Transaction<R::Model>,
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
        if let Some(sync_id) =
            apply_operation::<R>(db, actor, transaction.id, operation, origin).await?
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
        notify::<R>(db, actor.workspace_id).await?;
    }
    Ok(last_sync_id)
}

/// Records a change the server made to a record outside of a transaction,
/// such as someone joining the workspace. Clients hear about it like any
/// other change.
pub async fn record_change<R: Rules>(
    db: &mut PgConnection,
    actor: Actor,
    model: R::Model,
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
    notify::<R>(db, actor.workspace_id).await?;
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
pub async fn notify<R: Rules>(db: &mut PgConnection, workspace_id: Uuid) -> Result<(), SyncError> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(R::NOTIFY_CHANNEL)
        .bind(workspace_id.to_string())
        .execute(&mut *db)
        .await?;
    Ok(())
}

type Applying<'a> = Pin<Box<dyn Future<Output = Result<Option<i64>, SyncError>> + Send + 'a>>;

/// Applies one operation and logs it, with what it brings along: its
/// cascade and the app's [`Rules::after_write`]. Returns the sync id of the
/// last change, or `None` if nothing changed.
pub fn apply_operation<'a, R: Rules>(
    db: &'a mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    operation: &'a Operation<R::Model>,
    origin: Origin,
) -> Applying<'a> {
    Box::pin(async move {
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
            R::check_allowed(db, actor, operation).await?;
        }

        match operation {
            Operation::Create { model, id, data } => {
                model.validate_create(data)?;
                let mut row = data.clone();
                row.insert("id".into(), json!(id));
                row.insert("workspace_id".into(), json!(actor.workspace_id));
                R::prepare_create(db, actor, *model, &mut row).await?;

                let record = insert::<R>(db, *model, row).await?;
                R::check_rules(db, actor, *model, *id).await?;
                let visible_to = owner(*model, &record);
                let mut last = log(
                    db,
                    actor,
                    transaction_id,
                    *model,
                    *id,
                    SyncAction::Insert,
                    Some(record.clone()),
                    visible_to,
                )
                .await?;
                if let Some(sync_id) =
                    R::after_write(db, actor, transaction_id, *model, None, &record).await?
                {
                    last = sync_id;
                }
                Ok(Some(last))
            }
            Operation::Update { model, id, data } => {
                model.validate_patch(data)?;
                if data.is_empty() {
                    return Ok(None);
                }
                let before = current(db, actor, *model, *id).await?;
                let mut data = data.clone();
                R::prepare_update(db, actor, *model, *id, before.as_ref(), &mut data).await?;
                let record = update::<R>(db, actor, *model, *id, &data)
                    .await?
                    .ok_or_else(|| SyncError::NotFound(model.name().into()))?;
                R::check_rules(db, actor, *model, *id).await?;
                let visible_to = owner(*model, &record);
                let mut last = log(
                    db,
                    actor,
                    transaction_id,
                    *model,
                    *id,
                    SyncAction::Update,
                    Some(record.clone()),
                    visible_to,
                )
                .await?;
                if let Some(sync_id) =
                    R::after_write(db, actor, transaction_id, *model, before.as_ref(), &record)
                        .await?
                {
                    last = sync_id;
                }
                Ok(Some(last))
            }
            Operation::Delete { model, id } => {
                // Detach or delete what depends on the record first, through
                // the log, so clients' copies stay exact.
                let mut last = None;
                for dependent in R::cascade(db, actor, *model, *id).await? {
                    if let Some(sync_id) =
                        apply_operation::<R>(db, actor, transaction_id, &dependent, Origin::Server)
                            .await?
                    {
                        last = Some(sync_id);
                    }
                }

                let Some(record) = current(db, actor, *model, *id).await? else {
                    return Ok(last);
                };
                R::before_delete(db, actor, transaction_id, *model, *id).await?;
                let sql = format!(
                    "DELETE FROM {} WHERE id = $1 AND workspace_id = $2",
                    model.table()
                );
                // Safe: the table name is a constant from the app's `Model`.
                sqlx::query(AssertSqlSafe(sql))
                    .bind(id)
                    .bind(actor.workspace_id)
                    .execute(&mut *db)
                    .await
                    .map_err(|e| SyncError::explained(e, R::constraint_words))?;
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
    })
}

/// A record as it is now, or `None` if it doesn't exist.
pub async fn current<M: Model>(
    db: &mut PgConnection,
    actor: Actor,
    model: M,
    id: Uuid,
) -> Result<Option<Value>, SyncError> {
    let sql = format!(
        "SELECT to_jsonb(t.*) FROM {} AS t WHERE t.id = $1 AND t.workspace_id = $2",
        model.table()
    );
    // Safe: the table name is a constant from the app's `Model`.
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_optional(&mut *db)
        .await?)
}

/// For records only one person syncs, that person.
fn owner<M: Model>(model: M, record: &Value) -> Option<Uuid> {
    if !model.private_to_user() {
        return None;
    }
    record
        .get("user_id")
        .and_then(Value::as_str)
        .and_then(|id| id.parse().ok())
}

/// Inserts `row` into the model's table, letting Postgres convert the JSON
/// values to column types. Columns not in `row` get their defaults. It isn't
/// logged: that's for the caller.
pub async fn insert<R: Rules>(
    db: &mut PgConnection,
    model: R::Model,
    row: Map<String, Value>,
) -> Result<Value, SyncError> {
    let table = model.table();
    let columns = column_list(&row)?;
    let sql = format!(
        "INSERT INTO {table} ({columns}) SELECT {columns} FROM jsonb_populate_record(NULL::{table}, $1) \
         RETURNING to_jsonb({table}.*)"
    );
    // Safe: the table name is a constant and `column_list` checked the columns.
    sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(Value::Object(row))
        .fetch_one(&mut *db)
        .await
        .map_err(|e| SyncError::explained(e, R::constraint_words))
}

/// Sets the fields in `data` on one record and returns the whole record, or
/// `None` if it doesn't exist.
async fn update<R: Rules>(
    db: &mut PgConnection,
    actor: Actor,
    model: R::Model,
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
    sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(Value::Object(data.clone()))
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_optional(&mut *db)
        .await
        .map_err(|e| SyncError::explained(e, R::constraint_words))
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

/// Appends a change to the log. It reaches clients once the database
/// transaction commits and [`notify`] has run in it.
#[allow(clippy::too_many_arguments)]
pub async fn log<M: Model>(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    model: M,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_stay_out_of_reach_of_sql() {
        let row = |key: &str| Map::from_iter([(key.to_string(), json!(1))]);
        assert_eq!(column_list(&row("title_2")).unwrap(), "title_2");
        for bad in ["Title", "2nd", "a b", "x;drop table users", ""] {
            assert!(matches!(column_list(&row(bad)), Err(SyncError::Invalid(_))));
        }
    }
}
