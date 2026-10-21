use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use postgresql_embedded::{PostgreSQL, Settings, Status};
use rand::RngExt;
use rand::distr::Alphanumeric;

use crate::Result;

/// Postgres run by the app from the archive compiled into it, with its data
/// in a folder of the app's own. It listens on loopback only and asks for a
/// password, kept beside the data where only this user can read it.
///
/// The folder holds `data` (the cluster), `password` and `runtime` (the
/// unpacked binaries).
pub struct LocalPostgres {
    postgres: PostgreSQL,
}

impl LocalPostgres {
    /// Starts Postgres on the data in `root`, creating it the first time, on
    /// `preferred_port` when it's free and another port when it isn't (0:
    /// any). A server left running by an app that didn't quit cleanly is stopped
    /// first, so this one starts on the same data.
    pub async fn start(root: &Path, preferred_port: u16) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let mut settings = Settings {
            installation_dir: root.join("runtime"),
            data_dir: root.join("data"),
            password_file: root.join("pgpass"),
            password: password(&root.join("password"))?,
            host: Ipv4Addr::LOCALHOST.to_string(),
            port: free_port(preferred_port),
            temporary: false,
            // The first start unpacks the binaries and creates the cluster.
            timeout: Some(Duration::from_secs(120)),
            ..Settings::default()
        };
        settings
            .configuration
            .insert("listen_addresses".into(), Ipv4Addr::LOCALHOST.to_string());
        // No socket in /tmp: other programs reach it only through the port.
        settings
            .configuration
            .insert("unix_socket_directories".into(), "''".into());

        let mut postgres = PostgreSQL::new(settings);
        postgres.setup().await?;
        if postgres.status() == Status::Started {
            // A pid file: the last run didn't stop it. If it's still
            // running this stops it; if it died, Postgres clears the file.
            if let Err(error) = postgres.stop().await {
                tracing::debug!("no earlier Postgres to stop: {error}");
            }
        }
        if let Err(error) = postgres.start().await {
            let pid_file = postgres.settings().data_dir.join("postmaster.pid");
            if !pid_file.exists() {
                return Err(error.into());
            }
            // Nothing could be stopped above, so the file is stale, but its
            // pid now belongs to another process: Postgres won't clear it.
            tracing::warn!("removing a stale postmaster.pid: {error}");
            std::fs::remove_file(pid_file)?;
            postgres.start().await?;
        }
        Ok(Self { postgres })
    }

    /// The address of `database`, with the password in it.
    pub fn url(&self, database: &str) -> String {
        self.postgres.settings().url(database)
    }

    /// The address of `database`, creating it if it doesn't exist yet.
    pub async fn database(&self, database: &str) -> Result<String> {
        if !self.postgres.database_exists(database).await? {
            self.postgres.create_database(database).await?;
        }
        Ok(self.url(database))
    }

    pub fn port(&self) -> u16 {
        self.postgres.settings().port
    }

    pub fn data_dir(&self) -> &Path {
        &self.postgres.settings().data_dir
    }

    /// Stops Postgres and waits until it has.
    pub async fn stop(self) -> Result<()> {
        self.postgres.stop().await?;
        Ok(())
    }
}

/// The superuser's password: made once, then read from `file` (0600).
fn password(file: &PathBuf) -> std::io::Result<String> {
    if let Ok(password) = std::fs::read_to_string(file) {
        return Ok(password.trim().to_string());
    }
    let password: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(file)?.write_all(password.as_bytes())?;
    Ok(password)
}

fn free_port(preferred: u16) -> u16 {
    [preferred, 0]
        .into_iter()
        .find_map(|port| {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).ok()?;
            Some(listener.local_addr().ok()?.port())
        })
        .unwrap_or(preferred)
}
