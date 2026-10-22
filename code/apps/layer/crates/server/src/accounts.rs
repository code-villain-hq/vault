//! Signing up, signing in and out, the signed-in user, and passwords.
//! The first person to come sets Layer up, with its first workspace; after
//! that, people join with an invite.

use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use layer_domain::{Actor, SyncAction};
use layer_domain::{
    ChangePasswordRequest, LoginRequest, Me, Role, SignupRequest, UpdateProfileRequest,
    UpdateWorkspaceRequest, UserInfo, WorkspaceInfo, check_password, clean_name, normalize_email,
    url_key_from,
};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::{
    CLIENT_HEADER, CurrentUser, SESSION_HEADER, clear_session_cookie, hash_password, hash_token,
    session_cookie, session_token, start_session, verify_password,
};
use crate::error::AppError;
use crate::limits::ClientAddress;
use crate::state::AppState;
use crate::{invites, members};

/// What a sign-up refused because of the email says: it has an account
/// already, but not in so many words, so nobody can find out which emails
/// have accounts.
pub const SIGNUP_REFUSED: &str =
    "Couldn't sign up with these details. If you already have an account, sign in instead.";

/// A sign-up refused because of the email, with the same status (422) as a
/// sign-up refused for anything else.
pub fn signup_refused() -> AppError {
    AppError::invalid(SIGNUP_REFUSED)
}

/// With Code Villain sign-in on, it's the only way in: no passwords.
fn passwords_open(state: &AppState) -> Result<(), AppError> {
    if state.on_this_mac {
        return Err(AppError::not_found("this Layer is the desktop app's own"));
    }
    match state.oidc_login {
        Some(_) => Err(AppError::not_found("sign in with Code Villain instead")),
        None => Ok(()),
    }
}

/// Whether anyone has an account yet. Until someone has, the sign-in page
/// offers to set Layer up.
pub async fn has_accounts(pool: &PgPool) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(pool)
        .await?)
}

/// Signs someone up, at most five times an hour from one client address
/// (every try counts). The first person sets Layer up with a workspace;
/// everyone after them comes with an invite.
pub async fn signup(
    State(state): State<AppState>,
    client: ClientAddress,
    headers: HeaderMap,
    Json(request): Json<SignupRequest>,
) -> Result<Response, AppError> {
    passwords_open(&state)?;
    state
        .signup_attempts
        .count(&client.key())
        .map_err(AppError::too_many_signups)?;
    let name = clean_name(&request.name, "name")?;
    let email = normalize_email(&request.email)?;
    check_password(&request.password)?;
    let workspace_name = match (&request.workspace_name, &request.invite_token) {
        (Some(workspace), None) => Some(clean_name(workspace, "workspace name")?),
        (None, Some(_)) => None,
        _ => {
            return Err(AppError::invalid(
                "give either a workspace name or an invite",
            ));
        }
    };
    let password_hash = hash_password(request.password).await?;

    let mut db = state.pool.begin().await?;
    if workspace_name.is_some() {
        // One sign-up at a time, so two people can't both be first.
        sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE")
            .execute(&mut *db)
            .await?;
        let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
            .fetch_one(&mut *db)
            .await?;
        if taken {
            return Err(AppError::forbidden(
                "Layer is set up already: sign in, or ask an admin for an invite",
            ));
        }
    }
    // The invite first, so whether it still works can't depend on the email.
    let invite = match &request.invite_token {
        Some(token) => Some(
            invites::find_valid(&mut *db, token)
                .await?
                .ok_or_else(invites::gone)?,
        ),
        None => None,
    };
    let user_id = Uuid::now_v7();
    let created = sqlx::query(
        "INSERT INTO users (id, email, name, password_hash) VALUES ($1, $2, $3, $4) ON CONFLICT (email) DO NOTHING",
    )
    .bind(user_id)
    .bind(&email)
    .bind(&name)
    .bind(&password_hash)
    .execute(&mut *db)
    .await?
    .rows_affected();
    if created == 0 {
        return Err(signup_refused());
    }

    match (workspace_name, invite) {
        (Some(workspace), _) => {
            create_workspace(&mut db, &workspace, user_id).await?;
        }
        (None, Some(invite)) => {
            add_member(&mut db, invite.workspace_id, user_id, invite.role).await?;
        }
        (None, None) => unreachable!("checked above"),
    }

    let token = start_session(&mut *db, user_id).await?;
    db.commit().await?;
    signed_in(&state, &headers, &token, user_id).await
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, AppError> {
    passwords_open(&state)?;
    let email = normalize_email(&request.email).unwrap_or_default();
    let attempt = state
        .password_attempts
        .begin(&email)
        .map_err(AppError::too_many)?;
    let account: Option<(Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let (user_id, hash) = account.map_or((None, None), |(id, hash)| (Some(id), hash));

    // Check a password even when there's no account, so both take as long.
    let matches = verify_password(request.password, hash).await;
    let Some(user_id) = user_id.filter(|_| matches) else {
        attempt.failed();
        return Err(AppError::unauthorized("wrong email or password"));
    };
    attempt.succeeded();

    let token = start_session(&state.pool, user_id).await?;
    signed_in(&state, &headers, &token, user_id).await
}

/// Ends the current session, if there is one, and clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(hash_token(&token))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(SET_COOKIE, clear_session_cookie(state.cookie_secure))],
    )
        .into_response())
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> Result<Json<Me>, AppError> {
    Ok(Json(load_me(&state.pool, user.id).await?))
}

/// `PATCH /api/w/{workspace}`: renames the workspace (admins only).
pub async fn rename_workspace(
    State(state): State<AppState>,
    member: crate::auth::Member,
    Json(request): Json<UpdateWorkspaceRequest>,
) -> Result<Json<Me>, AppError> {
    member.require_admin()?;
    let name = clean_name(&request.name, "workspace name")?;
    sqlx::query("UPDATE workspaces SET name = $2 WHERE id = $1")
        .bind(member.actor.workspace_id)
        .bind(&name)
        .execute(&state.pool)
        .await?;
    Ok(Json(load_me(&state.pool, member.actor.user_id).await?))
}

/// `PUT /api/me`: changes your name, everywhere you're a member.
pub async fn update_profile(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<Me>, AppError> {
    let name = clean_name(&request.name, "name")?;
    let mut db = state.pool.begin().await?;
    sqlx::query("UPDATE users SET name = $2 WHERE id = $1")
        .bind(user.id)
        .bind(&name)
        .execute(&mut *db)
        .await?;
    let workspaces: Vec<Uuid> =
        // In a fixed order, so two renames at once can't lock workspaces in
        // opposite orders.
        sqlx::query_scalar(
            "SELECT workspace_id FROM workspace_members WHERE user_id = $1 ORDER BY workspace_id",
        )
            .bind(user.id)
            .fetch_all(&mut *db)
            .await?;
    for workspace_id in workspaces {
        let actor = Actor {
            user_id: user.id,
            workspace_id,
        };
        members::log_user(&mut db, actor, user.id, SyncAction::Update).await?;
    }
    db.commit().await?;
    Ok(Json(load_me(&state.pool, user.id).await?))
}

/// Changes the password and signs out every other session.
pub async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    passwords_open(&state)?;
    check_password(&request.new_password)?;
    let (email, hash): (String, Option<String>) =
        sqlx::query_as("SELECT email, password_hash FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&state.pool)
            .await?;
    // An account without a password (made by an admin) sets one.
    if hash.is_some() {
        // The same count as signing in to the account.
        let attempt = state
            .password_attempts
            .begin(&email)
            .map_err(AppError::too_many)?;
        // Not 401: that would tell the app the session has ended.
        if !verify_password(request.current_password, hash).await {
            attempt.failed();
            return Err(AppError::invalid("current password is wrong"));
        }
        attempt.succeeded();
    }

    let new_hash = hash_password(request.new_password).await?;
    let mut db = state.pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(new_hash)
        .bind(user.id)
        .execute(&mut *db)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2")
        .bind(user.id)
        .bind(&user.token_hash)
        .execute(&mut *db)
        .await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sets a new random password and ends every session; for the
/// `reset-password` command, until Layer can send reset emails.
pub async fn reset_password(pool: &PgPool, email: &str) -> anyhow::Result<String> {
    let email = normalize_email(email)?;
    let password = readable_password();
    let hash = hash_password(password.clone())
        .await
        .map_err(|_| anyhow::anyhow!("couldn't hash the password"))?;

    let mut db = pool.begin().await?;
    let user_id: Option<Uuid> =
        sqlx::query_scalar("UPDATE users SET password_hash = $1 WHERE email = $2 RETURNING id")
            .bind(hash)
            .bind(&email)
            .fetch_optional(&mut *db)
            .await?;
    let user_id =
        user_id.ok_or_else(|| anyhow::anyhow!("nobody has an account with the email {email}"))?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *db)
        .await?;
    db.commit().await?;
    Ok(password)
}

/// Makes a workspace with someone who has an account as its admin, from the
/// server (`layer-server create-workspace`): with Code Villain sign-in on,
/// sign-up is closed, so this is how a server gets its first workspace.
/// Returns the workspace's URL key.
pub async fn create_workspace_for(
    pool: &PgPool,
    name: &str,
    email: &str,
) -> anyhow::Result<String> {
    let name = clean_name(name, "workspace name")?;
    let email = normalize_email(email)?;
    let mut db = pool.begin().await?;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&mut *db)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no person has an account with the email {email}: they sign in to Layer first"
            )
        })?;
    let workspace_id = create_workspace(&mut db, &name, user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let url_key = sqlx::query_scalar("SELECT url_key FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&mut *db)
        .await?;
    db.commit().await?;
    Ok(url_key)
}

/// 20 random letters and digits in groups of four, avoiding look-alikes.
fn readable_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 20];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    let chars: Vec<char> = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    chars
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// The response to signing in: who you are, and the session as a cookie,
/// or for the desktop app as a header it keeps in the keychain.
pub async fn signed_in(
    state: &AppState,
    headers: &HeaderMap,
    token: &str,
    user_id: Uuid,
) -> Result<Response, AppError> {
    let me = load_me(&state.pool, user_id).await?;
    if headers.get(CLIENT_HEADER).is_some_and(|v| v == "desktop") {
        return Ok(([(SESSION_HEADER, token)], Json(me)).into_response());
    }
    Ok((
        [(SET_COOKIE, session_cookie(token, state.cookie_secure))],
        Json(me),
    )
        .into_response())
}

pub async fn load_me(pool: &PgPool, user_id: Uuid) -> Result<Me, AppError> {
    let (id, email, name): (Uuid, String, String) =
        sqlx::query_as("SELECT id, email, name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT w.id, w.name, w.url_key, m.role FROM workspace_members m \
         JOIN workspaces w ON w.id = m.workspace_id WHERE m.user_id = $1 ORDER BY m.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let workspaces = rows
        .into_iter()
        .map(|(id, name, url_key, role)| {
            let role = Role::parse(&role)
                .ok_or_else(|| AppError::internal(format!("unknown role {role:?}")))?;
            Ok(WorkspaceInfo {
                id,
                name,
                url_key,
                role,
            })
        })
        .collect::<Result<_, AppError>>()?;
    Ok(Me {
        user: UserInfo { id, email, name },
        workspaces,
    })
}

/// Creates a workspace with `admin` as its first member. Its URL key comes
/// from the name, with a number added if the key is taken.
pub async fn create_workspace(
    db: &mut PgConnection,
    name: &str,
    admin: Uuid,
) -> Result<Uuid, AppError> {
    let base = url_key_from(name);
    for attempt in 1..=100 {
        let url_key = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let id = Uuid::now_v7();
        let created =
            sqlx::query("INSERT INTO workspaces (id, name, url_key) VALUES ($1, $2, $3) ON CONFLICT (url_key) DO NOTHING")
                .bind(id)
                .bind(name)
                .bind(&url_key)
                .execute(&mut *db)
                .await?
                .rows_affected();
        if created == 1 {
            add_member(&mut *db, id, admin, Role::Admin).await?;
            return Ok(id);
        }
    }
    Err(AppError::conflict(
        "couldn't find a free URL for that workspace name",
    ))
}

/// Adds someone to a workspace. Someone already in it keeps their role.
pub async fn add_member(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    role: Role,
) -> Result<(), AppError> {
    let added = sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(role.as_str())
    .execute(&mut *db)
    .await?
    .rows_affected();
    if added == 1 {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        members::log_user(db, actor, user_id, SyncAction::Insert).await?;
    }
    Ok(())
}
