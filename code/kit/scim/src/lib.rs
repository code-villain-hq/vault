//! People and groups from the Code Villain account, for any app's server.
//!
//! Pocket ID pushes the people and groups an app's sign-in client allows to
//! `/api/scim/v2` (SCIM 2.0), after every change and at least once an hour.
//! This crate keeps them (`schema.sql`) and turns them into workspace roles:
//! a workspace that follows groups ([`follow_groups`]) has the people of its
//! users group (`forge-users` unless the settings name another) as members,
//! of its admins group as admins and of its guests group as guests. The app
//! does the joining and leaving itself, through [`Directory`], so its sync
//! records and its own rules (teams, grants) follow.
//!
//! What Pocket ID sends: it lists `/Users` and `/Groups` (`startIndex`,
//! `count`), matches them to its own by `externalId`, then creates (POST),
//! replaces (PUT) or deletes (DELETE) each one. A group's members are the
//! ids this server gave its users. It never uses PATCH or filters, and it
//! sends a group's friendly name, not its name: [`group_key`] makes a name
//! of it.

mod reconcile;
mod resources;
mod store;

use std::future::Future;
use std::marker::PhantomData;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::de::DeserializeOwned;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use reconcile::{Groups, follow_groups, role_source};
pub use store::{group_key, group_names, in_group};

use resources::{GroupIn, ListQuery, UserIn};

/// A role in a workspace, the same three in every app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Guest,
    Member,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Guest => "guest",
        }
    }

    pub fn parse(role: &str) -> Option<Role> {
        match role {
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            "guest" => Some(Role::Guest),
            _ => None,
        }
    }
}

/// What the app does when the groups change who is in a workspace. Each runs
/// in the transaction of the change; an error rolls it back.
pub trait Directory: Send + Sync + 'static {
    /// Puts someone in a workspace with a role, as accepting an invite does.
    fn add_member(
        db: &mut PgConnection,
        workspace: Uuid,
        user: Uuid,
        role: Role,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Changes the role of someone already in the workspace.
    fn change_role(
        db: &mut PgConnection,
        workspace: Uuid,
        user: Uuid,
        role: Role,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Takes someone out of the workspace, as an admin's "Remove" does.
    fn remove_member(
        db: &mut PgConnection,
        workspace: Uuid,
        user: Uuid,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// After the groups of these people changed, and their workspaces with
    /// them: Forge puts them in the teams linked to their groups.
    fn groups_changed(
        _db: &mut PgConnection,
        _users: &[Uuid],
    ) -> impl Future<Output = anyhow::Result<()>> + Send {
        async { Ok(()) }
    }

    /// Whether someone stays in workspaces without a group: Forge's agents,
    /// who are added in Forge.
    fn stays(
        _db: &mut PgConnection,
        _user: Uuid,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send {
        async { Ok(false) }
    }

    /// Why an email can't be a person's here, if it can't: Forge keeps its
    /// agents' addresses for agents.
    fn refuses_email(_email: &str) -> Option<&'static str> {
        None
    }
}

struct Scim<D> {
    pool: PgPool,
    token: String,
    directory: PhantomData<fn() -> D>,
}

impl<D> Clone for Scim<D> {
    fn clone(&self) -> Self {
        Scim {
            pool: self.pool.clone(),
            token: self.token.clone(),
            directory: PhantomData,
        }
    }
}

/// The SCIM endpoint, for `/api/scim/v2`. Every call needs
/// `Authorization: Bearer <token>`, the token set in Pocket ID.
pub fn router<D: Directory, S: Clone + Send + Sync + 'static>(
    pool: PgPool,
    token: String,
) -> Router<S> {
    let scim = Scim::<D> {
        pool,
        token,
        directory: PhantomData,
    };
    Router::new()
        .route("/ServiceProviderConfig", get(service_provider_config::<D>))
        .route("/Users", get(list_users::<D>).post(create_user::<D>))
        .route(
            "/Users/{id}",
            get(get_user::<D>)
                .put(replace_user::<D>)
                .delete(delete_user::<D>),
        )
        .route("/Groups", get(list_groups::<D>).post(create_group::<D>))
        .route(
            "/Groups/{id}",
            get(get_group::<D>)
                .put(replace_group::<D>)
                .delete(delete_group::<D>),
        )
        .with_state(scim)
}

/// A SCIM error: its status, what went wrong, and SCIM's word for it.
#[derive(Debug)]
pub struct ScimError {
    status: StatusCode,
    detail: String,
    scim_type: Option<&'static str>,
}

impl ScimError {
    fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        ScimError {
            status,
            detail: detail.into(),
            scim_type: None,
        }
    }

    fn typed(status: StatusCode, scim_type: &'static str, detail: impl Into<String>) -> Self {
        ScimError {
            status,
            detail: detail.into(),
            scim_type: Some(scim_type),
        }
    }

    fn not_found(what: &str) -> Self {
        ScimError::new(StatusCode::NOT_FOUND, format!("no such {what}"))
    }
}

impl From<sqlx::Error> for ScimError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "SCIM request failed");
        ScimError::new(StatusCode::INTERNAL_SERVER_ERROR, "the server failed")
    }
}

impl From<anyhow::Error> for ScimError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(%error, "SCIM change failed");
        ScimError::new(StatusCode::INTERNAL_SERVER_ERROR, "the server failed")
    }
}

impl IntoResponse for ScimError {
    fn into_response(self) -> Response {
        let mut body = serde_json::json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:Error"],
            "status": self.status.as_u16().to_string(),
            "detail": self.detail,
        });
        if let Some(scim_type) = self.scim_type {
            body["scimType"] = scim_type.into();
        }
        reply(self.status, body)
    }
}

type Reply = Result<Response, ScimError>;

fn reply(status: StatusCode, body: serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/scim+json")],
        body.to_string(),
    )
        .into_response()
}

/// Checks the bearer token, in time that doesn't depend on how much of it matches.
fn authorize<D>(scim: &Scim<D>, headers: &HeaderMap) -> Result<(), ScimError> {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .as_bytes();
    let expected = scim.token.as_bytes();
    let same = given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0;
    if same && !expected.is_empty() {
        Ok(())
    } else {
        Err(ScimError::new(
            StatusCode::UNAUTHORIZED,
            "a valid bearer token is needed",
        ))
    }
}

/// Pocket ID sends `application/scim+json`, which axum's `Json` refuses.
fn body<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, ScimError> {
    serde_json::from_slice(bytes).map_err(|e| {
        ScimError::typed(
            StatusCode::BAD_REQUEST,
            "invalidSyntax",
            format!("couldn't read the request: {e}"),
        )
    })
}

fn parse_id(id: &str, what: &str) -> Result<Uuid, ScimError> {
    Uuid::parse_str(id).map_err(|_| ScimError::not_found(what))
}

async fn service_provider_config<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
) -> Reply {
    authorize(&scim, &headers)?;
    Ok(reply(StatusCode::OK, resources::service_provider_config()))
}

async fn list_users<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Reply {
    authorize(&scim, &headers)?;
    let mut db = scim.pool.acquire().await?;
    let (total, users) = store::list_users(&mut db, query.offset(), query.limit()).await?;
    Ok(reply(
        StatusCode::OK,
        resources::list(
            total,
            query.start(),
            users.iter().map(resources::user).collect(),
        ),
    ))
}

async fn get_user<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Reply {
    authorize(&scim, &headers)?;
    let id = parse_id(&id, "user")?;
    let mut db = scim.pool.acquire().await?;
    let user = store::user(&mut db, id)
        .await?
        .ok_or_else(|| ScimError::not_found("user"))?;
    Ok(reply(StatusCode::OK, resources::user(&user)))
}

async fn create_user<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Reply {
    authorize(&scim, &headers)?;
    let user: UserIn = body(&bytes)?;
    let mut db = scim.pool.begin().await?;
    store::lock(&mut db).await?;
    let id = store::create_user(&mut db, &user, D::refuses_email).await?;
    reconcile::people::<D>(&mut db, &[id]).await?;
    let created = store::user(&mut db, id).await?.expect("just created");
    db.commit().await?;
    Ok(reply(StatusCode::CREATED, resources::user(&created)))
}

async fn replace_user<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    bytes: Bytes,
) -> Reply {
    authorize(&scim, &headers)?;
    let id = parse_id(&id, "user")?;
    let user: UserIn = body(&bytes)?;
    let mut db = scim.pool.begin().await?;
    store::lock(&mut db).await?;
    if !store::replace_user(&mut db, id, &user).await? {
        return Err(ScimError::not_found("user"));
    }
    if !user.active.unwrap_or(true) {
        store::end_sessions(&mut db, id).await?;
    }
    reconcile::people::<D>(&mut db, &[id]).await?;
    let replaced = store::user(&mut db, id).await?.expect("just replaced");
    db.commit().await?;
    Ok(reply(StatusCode::OK, resources::user(&replaced)))
}

/// The person no longer reaches this app: they leave the workspaces their
/// groups put them in, and are signed out. Their account stays, for history
/// and for invites.
async fn delete_user<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimError> {
    authorize(&scim, &headers)?;
    let id = parse_id(&id, "user")?;
    let mut db = scim.pool.begin().await?;
    store::lock(&mut db).await?;
    if !store::delete_user(&mut db, id).await? {
        return Err(ScimError::not_found("user"));
    }
    store::end_sessions(&mut db, id).await?;
    reconcile::people::<D>(&mut db, &[id]).await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_groups<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Reply {
    authorize(&scim, &headers)?;
    let mut db = scim.pool.acquire().await?;
    let (total, groups) = store::list_groups(&mut db, query.offset(), query.limit()).await?;
    Ok(reply(
        StatusCode::OK,
        resources::list(
            total,
            query.start(),
            groups.iter().map(resources::group).collect(),
        ),
    ))
}

async fn get_group<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Reply {
    authorize(&scim, &headers)?;
    let id = parse_id(&id, "group")?;
    let mut db = scim.pool.acquire().await?;
    let group = store::group(&mut db, id)
        .await?
        .ok_or_else(|| ScimError::not_found("group"))?;
    Ok(reply(StatusCode::OK, resources::group(&group)))
}

async fn create_group<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Reply {
    authorize(&scim, &headers)?;
    let group: GroupIn = body(&bytes)?;
    let mut db = scim.pool.begin().await?;
    store::lock(&mut db).await?;
    let id = store::create_group(&mut db, &group).await?;
    let members = store::group_member_ids(&mut db, id).await?;
    reconcile::people::<D>(&mut db, &members).await?;
    let created = store::group(&mut db, id).await?.expect("just created");
    db.commit().await?;
    Ok(reply(StatusCode::CREATED, resources::group(&created)))
}

async fn replace_group<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    bytes: Bytes,
) -> Reply {
    authorize(&scim, &headers)?;
    let id = parse_id(&id, "group")?;
    let group: GroupIn = body(&bytes)?;
    let mut db = scim.pool.begin().await?;
    store::lock(&mut db).await?;
    let mut affected = store::group_member_ids(&mut db, id).await?;
    if !store::replace_group(&mut db, id, &group).await? {
        return Err(ScimError::not_found("group"));
    }
    affected.extend(store::group_member_ids(&mut db, id).await?);
    affected.sort();
    affected.dedup();
    reconcile::people::<D>(&mut db, &affected).await?;
    let replaced = store::group(&mut db, id).await?.expect("just replaced");
    db.commit().await?;
    Ok(reply(StatusCode::OK, resources::group(&replaced)))
}

async fn delete_group<D: Directory>(
    State(scim): State<Scim<D>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimError> {
    authorize(&scim, &headers)?;
    let id = parse_id(&id, "group")?;
    let mut db = scim.pool.begin().await?;
    store::lock(&mut db).await?;
    let members = store::group_member_ids(&mut db, id).await?;
    if !store::delete_group(&mut db, id).await? {
        return Err(ScimError::not_found("group"));
    }
    reconcile::people::<D>(&mut db, &members).await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
