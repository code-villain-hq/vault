//! Adding and removing the mail accounts and the calendar server. Each is
//! signed in to first, so a wrong password shows on the form; its password
//! goes to the keychain ([`crate::secrets`]) and the record syncs without
//! it.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use postrider_domain::{
    Actor, Calendar, CalendarConnect, HUES, MailAccount, MailAccountConnect, ModelKind, Operation,
    Transaction,
};
use postrider_sync::Origin;
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Member;
use crate::caldav;
use crate::error::AppError;
use crate::imap::{self, Login};
use crate::state::AppState;

/// `POST /api/w/{workspace}/accounts`: "Add an account". Signs in to it,
/// keeps the password, and starts reading its Inbox. Adding an address
/// that's already here again replaces it, with the new password.
pub async fn add_account(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<MailAccountConnect>,
) -> Result<Json<MailAccount>, AppError> {
    member.require_admin()?;
    let request = request.cleaned()?;
    let port = request.port.unwrap_or(993);
    let username = request.username.clone().unwrap_or_default();
    let mut session = imap::sign_in(&Login {
        host: &request.host,
        port: u16::try_from(port).map_err(|_| AppError::invalid("A port is 1 to 65535."))?,
        username: &username,
        password: &request.password,
    })
    .await
    .map_err(|failure| AppError::refused(failure.message()))?;
    let _ = session.logout().await;

    state
        .secrets
        .set(&request.address, &request.password)
        .await
        .map_err(|e| AppError::internal(format!("couldn't keep the password: {e}")))?;

    let actor = member.actor;
    let mut db = state.pool.begin().await?;
    postrider_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let existing: Option<(Uuid, i32, i32)> = sqlx::query_as(
        "SELECT id, hue, position FROM mail_accounts WHERE workspace_id = $1 AND address = $2",
    )
    .bind(actor.workspace_id)
    .bind(&request.address)
    .fetch_optional(&mut *db)
    .await?;
    let hue = match existing {
        Some((_, hue, _)) => hue,
        None => free_hue(&mut db, actor.workspace_id).await?,
    };
    let id = Uuid::now_v7();
    let mut data = Map::from_iter([
        ("name".into(), json!(request.name)),
        ("address".into(), json!(request.address)),
        ("host".into(), json!(request.host)),
        ("port".into(), json!(port)),
        ("username".into(), json!(username)),
        ("hue".into(), json!(hue)),
    ]);
    let mut operations = Vec::new();
    if let Some((old, _, position)) = existing {
        data.insert("position".into(), json!(position));
        operations.push(Operation::Delete {
            model: ModelKind::MailAccount,
            id: old,
        });
    }
    operations.push(Operation::Create {
        model: ModelKind::MailAccount,
        id,
        data,
    });
    apply(&mut db, actor, operations).await?;
    let account = load_account(&mut db, id).await?;
    db.commit().await?;

    if let Some((old, _, _)) = existing {
        state.mail.forget(old);
    }
    state.mail.watch(account.clone());
    Ok(Json(account))
}

/// `DELETE /api/w/{workspace}/accounts/{id}`: stops reading an account and
/// forgets its password. Its mail stays on its server.
pub async fn remove_account(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let actor = member.actor;
    let mut db = state.pool.begin().await?;
    postrider_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let address: Option<String> =
        sqlx::query_scalar("SELECT address FROM mail_accounts WHERE id = $1 AND workspace_id = $2")
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_optional(&mut *db)
            .await?;
    let Some(address) = address else {
        return Err(AppError::not_found("account not found"));
    };
    apply(
        &mut db,
        actor,
        vec![Operation::Delete {
            model: ModelKind::MailAccount,
            id,
        }],
    )
    .await?;
    db.commit().await?;
    state.mail.forget(id);
    // Another workspace on this Mac may read the same address.
    let elsewhere: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM mail_accounts WHERE address = $1)")
            .bind(&address)
            .fetch_one(&state.pool)
            .await?;
    if !elsewhere {
        state
            .secrets
            .forget(&address)
            .await
            .map_err(|e| AppError::internal(format!("couldn't forget the password: {e}")))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{workspace}/calendar`: "Add a calendar". Signs in to the
/// server and finds its calendars, then keeps it, in place of one before.
pub async fn add_calendar(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<CalendarConnect>,
) -> Result<Json<Calendar>, AppError> {
    member.require_admin()?;
    let request = request.cleaned()?;
    let server = caldav::Server::new(&request.url, &request.username, &request.password)
        .map_err(AppError::invalid)?;
    let found = server.calendars().await.map_err(AppError::refused)?;
    if found.is_empty() {
        return Err(AppError::refused(format!(
            "{} has no calendars for {}.",
            server.host(),
            request.username
        )));
    }
    state
        .secrets
        .set(&request.url, &request.password)
        .await
        .map_err(|e| AppError::internal(format!("couldn't keep the password: {e}")))?;

    let actor = member.actor;
    let mut db = state.pool.begin().await?;
    postrider_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let mut operations = Vec::new();
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM calendars WHERE workspace_id = $1")
            .bind(actor.workspace_id)
            .fetch_optional(&mut *db)
            .await?;
    if let Some(old) = existing {
        operations.push(Operation::Delete {
            model: ModelKind::Calendar,
            id: old,
        });
    }
    let id = Uuid::now_v7();
    operations.push(Operation::Create {
        model: ModelKind::Calendar,
        id,
        data: Map::from_iter([
            ("url".into(), json!(request.url)),
            ("username".into(), json!(request.username)),
        ]),
    });
    apply(&mut db, actor, operations).await?;
    let calendar: Value = sqlx::query_scalar("SELECT to_jsonb(c.*) FROM calendars c WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *db)
        .await?;
    db.commit().await?;
    Ok(Json(
        serde_json::from_value(calendar).map_err(AppError::internal)?,
    ))
}

/// `DELETE /api/w/{workspace}/calendar`: stops reading the calendar server.
pub async fn remove_calendar(
    State(state): State<AppState>,
    member: Member,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let actor = member.actor;
    let mut db = state.pool.begin().await?;
    postrider_sync::lock_workspace(&mut db, actor.workspace_id).await?;
    let existing: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, url FROM calendars WHERE workspace_id = $1")
            .bind(actor.workspace_id)
            .fetch_optional(&mut *db)
            .await?;
    let Some((id, url)) = existing else {
        return Ok(StatusCode::NO_CONTENT);
    };
    apply(
        &mut db,
        actor,
        vec![Operation::Delete {
            model: ModelKind::Calendar,
            id,
        }],
    )
    .await?;
    db.commit().await?;
    let elsewhere: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM calendars WHERE url = $1)")
            .bind(&url)
            .fetch_one(&state.pool)
            .await?;
    if !elsewhere {
        state
            .secrets
            .forget(&url)
            .await
            .map_err(|e| AppError::internal(format!("couldn't forget the password: {e}")))?;
    }
    Ok(StatusCode::NO_CONTENT)
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
    postrider_sync::apply_in(db, actor, &transaction, Origin::Server).await?;
    Ok(())
}

async fn load_account(db: &mut PgConnection, id: Uuid) -> Result<MailAccount, AppError> {
    let row: Value = sqlx::query_scalar("SELECT to_jsonb(a.*) FROM mail_accounts a WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *db)
        .await?;
    serde_json::from_value(row).map_err(AppError::internal)
}

/// The first hue no account in the workspace wears yet, so each account's
/// mail and events look like its own; after six, they come round again.
async fn free_hue(db: &mut PgConnection, workspace_id: Uuid) -> Result<i32, AppError> {
    let taken: Vec<i32> =
        sqlx::query_scalar("SELECT hue FROM mail_accounts WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_all(&mut *db)
            .await?;
    let count = i32::try_from(taken.len()).unwrap_or(0);
    Ok((1..=HUES)
        .find(|hue| !taken.contains(hue))
        .unwrap_or(count % HUES + 1))
}
