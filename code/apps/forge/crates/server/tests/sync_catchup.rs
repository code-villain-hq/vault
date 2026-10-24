//! Revocation is checked inside catch-up, not only after the backlog drains.
mod common;

use axum::{Router, extract::WebSocketUpgrade, routing::get};
use code_villain_sync::Position;
use forge_domain::Actor;
use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[sqlx::test(migrations = "../../migrations")]
async fn catchup_checks_access_again_before_sending_the_next_page(pool: PgPool) {
    let app = common::App::new(pool.clone());
    let (_, workspace) = app.signup("Ada", "Acme").await;
    let actor = Actor {
        user_id: app.person("Ada").await,
        workspace_id: workspace.parse().unwrap(),
    };
    let snapshot = forge_sync::bootstrap(&pool, actor).await.unwrap();
    sqlx::query(
        "INSERT INTO sync_log (workspace_id, transaction_id, actor_id, model, model_id, action, data) \
         SELECT $1, gen_random_uuid(), $2, 'Issue', gen_random_uuid(), 'insert', '{}'::jsonb \
         FROM generate_series(1, 1001)",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();
    let router = Router::new().route(
        "/ws",
        get(move |ws: WebSocketUpgrade| {
            let pool = pool.clone();
            std::future::ready(ws.on_upgrade(move |socket| async move {
                let (_wake, receiver) = tokio::sync::broadcast::channel(1);
                let mut checks = 0;
                code_villain_sync::follow::<forge_sync::Forge, _, _>(
                    socket,
                    &pool,
                    receiver,
                    actor,
                    Position {
                        after: snapshot.sync_id,
                        epoch: Some(snapshot.epoch),
                    },
                    || {
                        checks += 1;
                        std::future::ready(checks <= 2)
                    },
                )
                .await
                .unwrap();
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(format!(
        "GET /ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    ).as_bytes()).await.unwrap();
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
    let (opcode, bytes) = frame(&mut stream).await;
    assert_eq!(opcode, 1);
    let delta: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(delta["entries"].as_array().unwrap().len(), 500);
    let (opcode, _) = frame(&mut stream).await;
    server.abort();
    assert_eq!(opcode, 8, "revocation closes the feed before a second page");
}

// A receive-only test client needs only unmasked server frames; using the
// existing Tokio TCP support avoids another WebSocket test dependency.
async fn frame(stream: &mut BufReader<TcpStream>) -> (u8, Vec<u8>) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let opcode = stream.read_u8().await.unwrap() & 0x0f;
        let size = stream.read_u8().await.unwrap();
        assert_eq!(size & 0x80, 0);
        let size = match size {
            126 => u64::from(stream.read_u16().await.unwrap()),
            127 => stream.read_u64().await.unwrap(),
            size => u64::from(size),
        };
        assert!(size < 1_000_000);
        let mut bytes = vec![0; size as usize];
        stream.read_exact(&mut bytes).await.unwrap();
        (opcode, bytes)
    })
    .await
    .unwrap()
}
