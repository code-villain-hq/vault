//! What was watched before workspaces moves into one, "Code Villain".

use raven_server::auth::hash_token;
use sqlx::PgPool;
use uuid::Uuid;

const BEFORE: &str = include_str!("../../../migrations/0001_init.sql");
const WORKSPACES: &str = include_str!("../../../migrations/0002_workspaces.sql");

#[sqlx::test(migrations = false)]
async fn existing_apps_people_and_data_move_to_one_workspace(pool: PgPool) {
    sqlx::raw_sql(BEFORE).execute(&pool).await.unwrap();
    let (ada, grace) = (Uuid::now_v7(), Uuid::now_v7());
    sqlx::query(
        "INSERT INTO users (id, email, name, created_at) VALUES \
         ($1, 'ada@example.com', 'Ada', now() - interval '1 day'), ($2, 'grace@example.com', 'Grace', now())",
    )
    .bind(ada)
    .bind(grace)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE apps SET ingest_key_hash = $1 WHERE key = 'forge'")
        .bind(hash_token("forge-key"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO spans (app, trace_id, span_id, name, kind, started_at, duration_ms) \
         VALUES ('forge', 't', 's', 'GET /', 'server', now(), 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO health_checks (app, ok, duration_ms) VALUES ('raven', true, 1)")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::raw_sql(WORKSPACES).execute(&pool).await.unwrap();

    let workspaces: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, name, url_key FROM workspaces")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(workspaces.len(), 1);
    let (workspace, name, url_key) = &workspaces[0];
    assert_eq!(
        (name.as_str(), url_key.as_str()),
        ("Code Villain", "code-villain")
    );

    let roles: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, role FROM workspace_members ORDER BY role")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(roles, [(ada, "admin".into()), (grace, "member".into())]);

    let apps: Vec<(String, Uuid, bool, Option<String>)> =
        sqlx::query_as("SELECT key, workspace_id, is_self, health_url FROM apps ORDER BY position")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(apps.iter().all(|(_, w, ..)| w == workspace));
    let raven = apps.iter().find(|(key, ..)| key == "raven").unwrap();
    assert!(raven.2, "Raven is itself");
    assert_eq!(raven.3, None, "Raven checks itself at its own address");
    assert_eq!(apps.iter().filter(|(_, _, is_self, _)| *is_self).count(), 1);

    let forge_key: Option<String> = sqlx::query_scalar(
        "SELECT a.key FROM app_keys k JOIN apps a ON a.id = k.app_id WHERE k.key_hash = $1",
    )
    .bind(hash_token("forge-key"))
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(forge_key.as_deref(), Some("forge"));

    let span_app: String =
        sqlx::query_scalar("SELECT a.key FROM spans s JOIN apps a ON a.id = s.app_id")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(span_app, "forge");
    let check_app: String =
        sqlx::query_scalar("SELECT a.key FROM health_checks h JOIN apps a ON a.id = h.app_id")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(check_app, "raven");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_raven_nobody_has_set_up_watches_nothing_yet(pool: PgPool) {
    let apps: i64 = sqlx::query_scalar("SELECT count(*) FROM apps")
        .fetch_one(&pool)
        .await
        .unwrap();
    let workspaces: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((apps, workspaces), (0, 0));
}
