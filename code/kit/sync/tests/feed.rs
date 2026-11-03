//! The live feed with several people on one workspace: each change reaches
//! everyone who may see it once and in order, nothing reaches someone who
//! was removed, and following a change costs few database queries.
//!
//! The clients are raw WebSocket connections over TCP: they only read the
//! server's frames, which are never masked, so no WebSocket crate is needed.

use std::cell::Cell;
use std::sync::Once;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use code_villain_sync::{
    Actor, GuestView, Model, Position, Rules, SyncError, ValidationError, Write, bootstrap,
    follow_upgrade,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{PgConnection, PgPool};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum Kind {
    Note,
}

impl Model for Kind {
    const ALL: &'static [Self] = &[Kind::Note];
    fn table(self) -> &'static str {
        "notes"
    }
    fn name(self) -> &'static str {
        "Note"
    }
    fn client_may(self, _write: Write) -> bool {
        true
    }
    fn private_to_user(self) -> bool {
        false
    }
    fn guest_view(self) -> GuestView {
        GuestView::Where("NOT t.secret")
    }
    fn validate_create(self, _data: &Map<String, Value>) -> Result<(), ValidationError> {
        Ok(())
    }
    fn validate_patch(self, _data: &Map<String, Value>) -> Result<(), ValidationError> {
        Ok(())
    }
}

struct Notes;

impl Rules for Notes {
    type Model = Kind;
    const NOTIFY_CHANNEL: &'static str = "notes_sync";

    async fn guest_access(db: &mut PgConnection, actor: Actor) -> Result<Option<i64>, SyncError> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT role, access_version FROM workspace_members \
             WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(actor.workspace_id)
        .bind(actor.user_id)
        .fetch_optional(&mut *db)
        .await?;
        let (role, version) = row.ok_or_else(|| SyncError::NotFound("workspace".into()))?;
        Ok((role == "guest").then_some(version))
    }
}

const TABLES: &str = "
CREATE TABLE workspace_members (
    workspace_id uuid NOT NULL REFERENCES workspaces (id),
    user_id uuid NOT NULL REFERENCES users (id),
    role text NOT NULL,
    access_version bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (workspace_id, user_id)
);
CREATE TABLE notes (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id),
    secret boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
";

/// A workspace with its people, served over a local feed.
struct World {
    pool: PgPool,
    workspace: Uuid,
    wake: broadcast::Sender<Uuid>,
    addr: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct Served {
    pool: PgPool,
    wake: broadcast::Sender<Uuid>,
    workspace: Uuid,
}

#[derive(Deserialize)]
struct Params {
    user: Uuid,
    after: i64,
    epoch: Uuid,
}

async fn feed(
    ws: WebSocketUpgrade,
    State(served): State<Served>,
    Query(params): Query<Params>,
) -> Response {
    let actor = Actor {
        user_id: params.user,
        workspace_id: served.workspace,
    };
    let pool = served.pool.clone();
    follow_upgrade::<Notes, _, _>(
        ws,
        "notes",
        served.pool,
        served.wake,
        actor,
        Position {
            after: params.after,
            epoch: Some(params.epoch),
        },
        // As the apps do: one query, before every change sent.
        move || {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (SELECT 1 FROM workspace_members \
                     WHERE workspace_id = $1 AND user_id = $2)",
                )
                .bind(actor.workspace_id)
                .bind(actor.user_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(false)
            }
        },
    )
}

impl World {
    async fn new(pool: PgPool) -> World {
        sqlx::raw_sql(include_str!("../schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(TABLES).execute(&pool).await.unwrap();
        let workspace = Uuid::now_v7();
        sqlx::query("INSERT INTO workspaces (id, name) VALUES ($1, 'Acme')")
            .bind(workspace)
            .execute(&pool)
            .await
            .unwrap();
        let (wake, _) = broadcast::channel(64);
        let router = Router::new().route("/ws", get(feed)).with_state(Served {
            pool: pool.clone(),
            wake: wake.clone(),
            workspace,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        World {
            pool,
            workspace,
            wake,
            addr,
            server,
        }
    }

    async fn person(&self, name: &str, role: &str) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(format!("{name}@acme.test"))
            .bind(name)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3)",
        )
        .bind(self.workspace)
        .bind(id)
        .bind(role)
        .execute(&self.pool)
        .await
        .unwrap();
        id
    }

    /// Opens `user`'s feed from a fresh snapshot.
    async fn connect(&self, user: Uuid) -> Client {
        let actor = Actor {
            user_id: user,
            workspace_id: self.workspace,
        };
        let snapshot = bootstrap::<Notes>(&self.pool, actor).await.unwrap();
        let addr = self.addr;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!(
                    "GET /ws?user={user}&after={}&epoch={} HTTP/1.1\r\nHost: {addr}\r\n\
                     Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\
                     Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
                    snapshot.sync_id, snapshot.epoch
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut stream = BufReader::new(stream);
        let mut status = String::new();
        stream.read_line(&mut status).await.unwrap();
        assert!(status.contains("101"), "{status}");
        loop {
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            if line == "\r\n" {
                break;
            }
        }
        Client {
            stream,
            sync_id: snapshot.sync_id,
        }
    }

    /// Writes a note and logs it, then wakes the feeds as `listen` would.
    async fn write(&self, by: Uuid, secret: bool) -> (Uuid, i64) {
        let (id, sync_id) = self.log(by, secret).await;
        self.wake();
        (id, sync_id)
    }

    async fn log(&self, by: Uuid, secret: bool) -> (Uuid, i64) {
        let id = Uuid::now_v7();
        let mut db = self.pool.begin().await.unwrap();
        let data: Value = sqlx::query_scalar(
            "INSERT INTO notes (id, workspace_id, secret) VALUES ($1, $2, $3) RETURNING to_jsonb(notes.*)",
        )
        .bind(id)
        .bind(self.workspace)
        .bind(secret)
        .fetch_one(&mut *db)
        .await
        .unwrap();
        let sync_id: i64 = sqlx::query_scalar(
            "INSERT INTO sync_log (workspace_id, transaction_id, actor_id, model, model_id, action, data) \
             VALUES ($1, gen_random_uuid(), $2, 'Note', $3, 'insert', $4) RETURNING sync_id",
        )
        .bind(self.workspace)
        .bind(by)
        .bind(id)
        .bind(data)
        .fetch_one(&mut *db)
        .await
        .unwrap();
        db.commit().await.unwrap();
        (id, sync_id)
    }

    /// Logs `n` notes in one transaction.
    async fn log_many(&self, by: Uuid, n: i32) -> Vec<(i64, Uuid)> {
        sqlx::query_as(
            "WITH new AS ( \
                 INSERT INTO notes (id, workspace_id) \
                 SELECT gen_random_uuid(), $1 FROM generate_series(1, $3) \
                 RETURNING * \
             ) \
             INSERT INTO sync_log (workspace_id, transaction_id, actor_id, model, model_id, action, data) \
             SELECT $1, gen_random_uuid(), $2, 'Note', new.id, 'insert', to_jsonb(new.*) FROM new \
             RETURNING sync_id, model_id",
        )
        .bind(self.workspace)
        .bind(by)
        .bind(n)
        .fetch_all(&self.pool)
        .await
        .map(|mut rows: Vec<(i64, Uuid)>| {
            rows.sort();
            rows
        })
        .unwrap()
    }

    fn wake(&self) {
        self.wake.send(self.workspace).unwrap();
    }

    async fn remove(&self, user: Uuid) {
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
            .bind(self.workspace)
            .bind(user)
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[derive(Debug, PartialEq)]
enum Heard {
    /// The ids of the records changed, and the position the client is at after.
    Delta(Vec<(i64, Uuid)>, i64),
    Resync,
    /// A close frame, or the connection dropped: either way the feed ended.
    Closed,
}

struct Client {
    stream: BufReader<TcpStream>,
    sync_id: i64,
}

impl Client {
    /// The next thing the server says, skipping pings and heartbeats.
    async fn next(&mut self) -> Heard {
        self.within(Duration::from_secs(3))
            .await
            .expect("the server said nothing")
    }

    /// The next thing the server says within `wait`, if anything.
    async fn within(&mut self, wait: Duration) -> Option<Heard> {
        loop {
            let Some((opcode, bytes)) = tokio::time::timeout(wait, frame(&mut self.stream))
                .await
                .ok()?
            else {
                return Some(Heard::Closed);
            };
            match opcode {
                1 => {
                    let message: Value = serde_json::from_slice(&bytes).unwrap();
                    match message["type"].as_str().unwrap() {
                        "delta" => {
                            let entries = message["entries"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|entry| {
                                    (
                                        entry["sync_id"].as_i64().unwrap(),
                                        entry["model_id"].as_str().unwrap().parse().unwrap(),
                                    )
                                })
                                .collect();
                            let sync_id = message["sync_id"].as_i64().unwrap();
                            assert!(sync_id > self.sync_id, "the cursor goes forward");
                            self.sync_id = sync_id;
                            return Some(Heard::Delta(entries, sync_id));
                        }
                        "resync" => return Some(Heard::Resync),
                        "alive" => continue,
                        other => panic!("unexpected message {other}"),
                    }
                }
                8 => return Some(Heard::Closed),
                9 | 10 => continue,
                other => panic!("unexpected opcode {other}"),
            }
        }
    }

    /// Every change heard until the client's position reaches `sync_id`,
    /// whichever deltas they come in: a feed that was still catching up when
    /// a change was written may send it with the catch-up.
    async fn until(&mut self, sync_id: i64) -> Vec<(i64, Uuid)> {
        let mut heard = Vec::new();
        while self.sync_id < sync_id {
            match self.next().await {
                Heard::Delta(entries, _) => heard.extend(entries),
                other => panic!("expected changes, heard {other:?}"),
            }
        }
        assert_eq!(
            self.sync_id, sync_id,
            "the position stops at the last change"
        );
        heard
    }

    /// Asserts the server says nothing more for a moment.
    async fn hears_nothing(&mut self) {
        assert_eq!(self.within(Duration::from_millis(300)).await, None);
    }
}

/// The next frame, or `None` once the connection is gone.
async fn frame(stream: &mut BufReader<TcpStream>) -> Option<(u8, Vec<u8>)> {
    let opcode = stream.read_u8().await.ok()? & 0x0f;
    let size = stream.read_u8().await.unwrap();
    assert_eq!(size & 0x80, 0, "server frames aren't masked");
    let size = match size {
        126 => u64::from(stream.read_u16().await.unwrap()),
        127 => stream.read_u64().await.unwrap(),
        size => u64::from(size),
    };
    assert!(size < 10_000_000);
    let mut bytes = vec![0; size as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    Some((opcode, bytes))
}

#[sqlx::test(migrations = false)]
async fn two_people_on_one_workspace_get_each_change_once_in_order(pool: PgPool) {
    let world = World::new(pool).await;
    let ada = world.person("Ada", "member").await;
    let bo = world.person("Bo", "admin").await;
    let mut ada_feed = world.connect(ada).await;
    let mut bo_feed = world.connect(bo).await;

    let first = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut bo_feed] {
        assert_eq!(feed.until(first.1).await, vec![(first.1, first.0)]);
    }

    // Several changes before the feeds wake, then two wakes: every change
    // arrives once, in order, and the second wake brings nothing again.
    let second = world.log(bo, false).await;
    let third = world.log(ada, true).await;
    world.wake();
    world.wake();
    for feed in [&mut ada_feed, &mut bo_feed] {
        assert_eq!(
            feed.until(third.1).await,
            vec![(second.1, second.0), (third.1, third.0)]
        );
        feed.hears_nothing().await;
    }

    // One after the other, each wake with its own change.
    let mut written = Vec::new();
    for n in 0..5 {
        let (id, sync_id) = world.write(if n % 2 == 0 { ada } else { bo }, false).await;
        written.push((sync_id, id));
    }
    let last = written.last().unwrap().0;
    for feed in [&mut ada_feed, &mut bo_feed] {
        assert_eq!(feed.until(last).await, written);
        feed.hears_nothing().await;
    }
}

#[sqlx::test(migrations = false)]
async fn a_burst_of_changes_arrives_in_pages_once_each(pool: PgPool) {
    let world = World::new(pool).await;
    let ada = world.person("Ada", "member").await;
    let bo = world.person("Bo", "member").await;
    let mut ada_feed = world.connect(ada).await;
    let mut bo_feed = world.connect(bo).await;
    let first = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut bo_feed] {
        feed.until(first.1).await;
    }

    // More than a feed sends at once: pages of 500, every change once. 700
    // are still shared by the feeds; after 1,201 each reads the log itself.
    for (count, expected_pages) in [(700, 2), (1_201, 3)] {
        let written = world.log_many(bo, count).await;
        world.wake();
        let last = written.last().unwrap().0;
        for feed in [&mut ada_feed, &mut bo_feed] {
            let mut pages = 0;
            let mut heard = Vec::new();
            while feed.sync_id < last {
                let Heard::Delta(entries, _) = feed.next().await else {
                    panic!("expected changes");
                };
                assert!(entries.len() <= 500);
                heard.extend(entries);
                pages += 1;
            }
            assert_eq!(pages, expected_pages);
            assert_eq!(heard, written);
            feed.hears_nothing().await;
        }
    }

    // And both carry on from there.
    let next = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut bo_feed] {
        assert_eq!(
            feed.next().await,
            Heard::Delta(vec![(next.1, next.0)], next.1)
        );
    }
}

#[sqlx::test(migrations = false)]
async fn a_removed_member_gets_nothing_more_while_the_others_carry_on(pool: PgPool) {
    let world = World::new(pool).await;
    let ada = world.person("Ada", "member").await;
    let bo = world.person("Bo", "member").await;
    let mut ada_feed = world.connect(ada).await;
    let mut bo_feed = world.connect(bo).await;

    let first = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut bo_feed] {
        assert_eq!(feed.until(first.1).await, vec![(first.1, first.0)]);
    }

    world.remove(bo).await;
    let second = world.write(ada, false).await;
    assert_eq!(
        ada_feed.next().await,
        Heard::Delta(vec![(second.1, second.0)], second.1)
    );
    assert_eq!(bo_feed.next().await, Heard::Closed);

    // Ada's feed reads on as before.
    let third = world.write(ada, false).await;
    assert_eq!(
        ada_feed.next().await,
        Heard::Delta(vec![(third.1, third.0)], third.1)
    );
}

#[sqlx::test(migrations = false)]
async fn a_removal_with_nothing_logged_ends_the_feed_on_the_next_wake(pool: PgPool) {
    let world = World::new(pool).await;
    let ada = world.person("Ada", "member").await;
    let bo = world.person("Bo", "member").await;
    let mut ada_feed = world.connect(ada).await;
    let mut bo_feed = world.connect(bo).await;
    let first = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut bo_feed] {
        feed.until(first.1).await;
    }

    world.remove(bo).await;
    world.wake();
    assert_eq!(bo_feed.next().await, Heard::Closed);
    ada_feed.hears_nothing().await;
}

#[sqlx::test(migrations = false)]
async fn a_change_read_for_someone_since_removed_is_never_sent(pool: PgPool) {
    let world = World::new(pool).await;
    let ada = world.person("Ada", "member").await;
    let bo = world.person("Bo", "member").await;
    let mut ada_feed = world.connect(ada).await;
    let mut bo_feed = world.connect(bo).await;
    let first = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut bo_feed] {
        feed.until(first.1).await;
    }

    // Ada's feed reads the change, which may serve Bo's too; Bo is removed
    // right after. Whatever his feed had read, it sends nothing written
    // after he went, and it ends.
    let second = world.write(ada, false).await;
    assert_eq!(
        ada_feed.next().await,
        Heard::Delta(vec![(second.1, second.0)], second.1)
    );
    world.remove(bo).await;
    let third = world.write(ada, false).await;
    loop {
        match bo_feed.next().await {
            Heard::Delta(entries, _) => assert!(entries.iter().all(|e| e.0 < third.1)),
            Heard::Closed => break,
            Heard::Resync => panic!("expected the feed to end"),
        }
    }
    assert_eq!(
        ada_feed.next().await,
        Heard::Delta(vec![(third.1, third.0)], third.1)
    );
}

#[sqlx::test(migrations = false)]
async fn a_guest_gets_only_what_they_see_next_to_a_member(pool: PgPool) {
    let world = World::new(pool).await;
    let ada = world.person("Ada", "member").await;
    let gus = world.person("Gus", "guest").await;
    let mut ada_feed = world.connect(ada).await;
    let mut gus_feed = world.connect(gus).await;
    let first = world.write(ada, false).await;
    for feed in [&mut ada_feed, &mut gus_feed] {
        assert_eq!(feed.until(first.1).await, vec![(first.1, first.0)]);
    }

    let open = world.log(ada, false).await;
    let secret = world.log(ada, true).await;
    world.wake();
    assert_eq!(
        ada_feed.until(secret.1).await,
        vec![(open.1, open.0), (secret.1, secret.0)]
    );
    // The secret one is left out, but the guest's position still moves past it.
    assert_eq!(gus_feed.until(secret.1).await, vec![(open.1, open.0)]);

    // A member made a guest starts over from a snapshot of what they see.
    sqlx::query("UPDATE workspace_members SET role = 'guest' WHERE user_id = $1")
        .bind(ada)
        .execute(&world.pool)
        .await
        .unwrap();
    let next = world.write(gus, false).await;
    assert_eq!(ada_feed.next().await, Heard::Resync);
    assert_eq!(
        gus_feed.next().await,
        Heard::Delta(vec![(next.1, next.0)], next.1)
    );

    // A guest whose access changes with nothing logged starts over too.
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 WHERE user_id = $1",
    )
    .bind(gus)
    .execute(&world.pool)
    .await
    .unwrap();
    world.wake();
    assert_eq!(gus_feed.next().await, Heard::Resync);
}

thread_local! {
    static QUERIES: Cell<usize> = const { Cell::new(0) };
}

/// The statements sqlx has run on this thread. `sqlx::test` runs the test,
/// the server and every feed on the test's thread, so this is all of them.
/// sqlx doesn't log `BEGIN`, so a transaction counts one less than it costs.
fn queries() -> usize {
    static COUNTING: Once = Once::new();
    // Once for every test thread: a subscriber set for one thread alone can
    // miss callsites other threads met first.
    COUNTING.call_once(|| tracing::subscriber::set_global_default(CountQueries).unwrap());
    QUERIES.with(Cell::get)
}

struct CountQueries;

impl tracing::Subscriber for CountQueries {
    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if metadata.target() == "sqlx::query" {
            tracing::subscriber::Interest::always()
        } else {
            tracing::subscriber::Interest::never()
        }
    }
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == "sqlx::query"
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if event.metadata().target() == "sqlx::query" {
            QUERIES.with(|count| count.set(count.get() + 1));
        }
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

/// What one write costs the feeds, in database statements, with several
/// people following the workspace. Run with `--nocapture` to see the numbers.
#[sqlx::test(migrations = false)]
async fn a_write_costs_each_follower_few_queries(pool: PgPool) {
    const PEOPLE: usize = 4;
    queries();

    let world = World::new(pool).await;
    let mut people = Vec::new();
    for n in 0..PEOPLE {
        people.push(world.person(&format!("P{n}"), "member").await);
    }
    let mut feeds = Vec::new();
    for person in &people {
        feeds.push(world.connect(*person).await);
    }
    // Every feed has caught up and is waiting before anything is counted.
    let first = world.write(people[0], false).await;
    for feed in &mut feeds {
        feed.until(first.1).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut per_write = Vec::new();
    for _ in 0..3 {
        let (id, sync_id) = world.log(people[1], false).await;
        let before = queries();
        world.wake();
        for feed in &mut feeds {
            assert_eq!(
                feed.next().await,
                Heard::Delta(vec![(sync_id, id)], sync_id)
            );
        }
        // Let the feeds finish whatever they do after sending.
        tokio::time::sleep(Duration::from_millis(300)).await;
        per_write.push(queries() - before);
    }
    eprintln!("statements logged per write with {PEOPLE} people following: {per_write:?}");
    assert!(
        per_write.iter().all(|&n| n > 0),
        "the count sees sqlx's statements"
    );
    // One shared read, and per person the app's check and their access.
    assert!(
        per_write.iter().all(|&n| n <= 2 * PEOPLE + 1),
        "{per_write:?} statements for one write to {PEOPLE} people"
    );
}
