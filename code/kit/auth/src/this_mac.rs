//! The one person of a desktop app's own server: no password, no real email.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Never a real address (`.invalid` can't be registered).
pub const EMAIL: &str = "me@this-mac.invalid";

/// The person on this Mac, once they've started here.
pub async fn person(pool: &PgPool) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(EMAIL)
        .fetch_optional(pool)
        .await
}

/// Insert or keep the this-Mac person; returns their id. `name` is already cleaned.
pub async fn ensure_person(db: &mut PgConnection, name: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO users (id, email, name) VALUES ($1, $2, $3)
         ON CONFLICT (email) DO UPDATE SET name = users.name RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(EMAIL)
    .bind(name)
    .fetch_one(&mut *db)
    .await
}
