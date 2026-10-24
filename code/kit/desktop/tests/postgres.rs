//! Postgres in the app: data survives a restart, and a crash. One test, so
//! two servers never race for the preferred port.

use std::os::unix::fs::PermissionsExt;

use code_villain_desktop::LocalPostgres;
use sqlx::{Connection, PgConnection, Row};

async fn write(postgres: &LocalPostgres, value: &str) {
    let url = postgres.database("app").await.unwrap();
    let mut db = PgConnection::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS notes (body text)")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO notes VALUES ($1)")
        .bind(value)
        .execute(&mut db)
        .await
        .unwrap();
}

async fn read(postgres: &LocalPostgres) -> Vec<String> {
    let url = postgres.database("app").await.unwrap();
    let mut db = PgConnection::connect(&url).await.unwrap();
    sqlx::query("SELECT body FROM notes")
        .fetch_all(&mut db)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

fn postmaster_pid(postgres: &LocalPostgres) -> String {
    let file = std::fs::read_to_string(postgres.data_dir().join("postmaster.pid")).unwrap();
    file.lines().next().unwrap().to_string()
}

#[tokio::test]
async fn keeps_its_data_to_itself_across_restarts_and_crashes() {
    let root = tempfile::tempdir().unwrap();

    let postgres = LocalPostgres::start(root.path(), 0).await.unwrap();
    let mut settings = PgConnection::connect(&postgres.url("postgres"))
        .await
        .unwrap();
    let fsync: String = sqlx::query_scalar("SHOW fsync")
        .fetch_one(&mut settings)
        .await
        .unwrap();
    assert_eq!(
        fsync, "on",
        "persistent workspaces must survive loss of the OS disk cache"
    );
    settings.close().await.unwrap();
    // Loopback only, and only with the password, kept where only we can read it.
    assert!(postgres.url("app").contains("@127.0.0.1:"));
    let wrong = format!(
        "postgresql://postgres:wrong@127.0.0.1:{}/postgres",
        postgres.port()
    );
    assert!(PgConnection::connect(&wrong).await.is_err());
    let password = std::fs::metadata(root.path().join("password")).unwrap();
    assert_eq!(password.permissions().mode() & 0o777, 0o600);

    write(&postgres, "first").await;
    postgres.stop().await.unwrap();

    // A quit that didn't stop Postgres: it's still running on the data.
    let postgres = LocalPostgres::start(root.path(), 0).await.unwrap();
    assert_eq!(read(&postgres).await, ["first"]);
    write(&postgres, "second").await;
    std::mem::forget(postgres);

    // Killed outright: the pid file stays behind.
    let postgres = LocalPostgres::start(root.path(), 0).await.unwrap();
    assert_eq!(read(&postgres).await, ["first", "second"]);
    let pid = postmaster_pid(&postgres);
    std::mem::forget(postgres);
    let killed = std::process::Command::new("kill")
        .args(["-9", &pid])
        .status();
    assert!(killed.unwrap().success());

    let postgres = LocalPostgres::start(root.path(), 0).await.unwrap();
    assert_eq!(read(&postgres).await, ["first", "second"]);
    postgres.stop().await.unwrap();
}

#[tokio::test]
async fn failed_start_and_stop_leave_the_owners_pid_file_alone() {
    let root = tempfile::tempdir().unwrap();
    let postgres = LocalPostgres::start(root.path(), 0).await.unwrap();
    postgres.stop().await.unwrap();
    // Simulate command failures without signalling or disrupting a real
    // owner. Only this disposable cluster's extracted pg_ctl is replaced.
    let runtime = root.path().join("runtime");
    let ctl = std::fs::read_dir(runtime)
        .unwrap()
        .flatten()
        .map(|entry| entry.path().join("bin/pg_ctl"))
        .find(|path| path.is_file())
        .unwrap();
    std::fs::write(ctl, "#!/bin/sh\nexit 1\n").unwrap();
    let pid = root.path().join("data/postmaster.pid");
    std::fs::write(&pid, "12345\n").unwrap();
    assert!(LocalPostgres::start(root.path(), 0).await.is_err());
    assert_eq!(std::fs::read_to_string(pid).unwrap(), "12345\n");
}
