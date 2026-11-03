//! The WebSocket clients follow the change log on.
//!
//! The client connects with the last sync id and epoch it has. The server
//! sends everything after that sync id, then each new change as it happens.
//! If the client's copy can't be caught up, the server asks it to resync.
//!
//! The feeds of one workspace share their reads of the log ([`Tail`]): a
//! change is read once for everyone following the workspace in this server,
//! and each feed then checks only its own viewer's access before sending it.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use sqlx::{FromRow, PgPool, Row};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use uuid::Uuid;

use crate::read::{LogRow, guest_sees_of, visible_entries};
use crate::scope::served_epoch;
use crate::{Actor, Rules, ServerMessage, current_sync_id, entries_after, workspace_epoch};

/// How many log entries to read at a time.
const PAGE: i64 = 500;
/// How many of a workspace's latest entries its feeds share. A feed further
/// behind reads the log itself.
const KEEP: usize = 2 * PAGE as usize;
/// How often to ping, say the feed is alive, check the session is still
/// valid, and look for changes in case a notification was missed.
const HEARTBEAT: Duration = Duration::from_secs(25);

/// Where a client's copy is: the last sync id it has seen, in which epoch.
pub struct Position {
    pub after: i64,
    pub epoch: Option<Uuid>,
}

/// Upgrades `ws` to `protocol` and runs [`follow`] on it. Subscribes to `wake`
/// before the catch-up so no change slips between the two.
pub fn follow_upgrade<R, F, Fut>(
    ws: WebSocketUpgrade,
    protocol: &'static str,
    pool: PgPool,
    wake: broadcast::Sender<Uuid>,
    actor: Actor,
    from: Position,
    still_allowed: F,
) -> Response
where
    R: Rules + 'static,
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = bool> + Send,
{
    // Selecting the protocol the desktop app offers next to its token, as
    // browsers require; the browser app offers none.
    ws.protocols([protocol])
        .on_upgrade(move |socket| async move {
            let wake = wake.subscribe();
            if let Err(e) = follow::<R, _, _>(socket, &pool, wake, actor, from, still_allowed).await
            {
                tracing::debug!(error = %e, "sync socket closed");
            }
        })
}

/// Sends `actor` the workspace's changes after `from` on `socket` until the
/// client leaves or `still_allowed` says they may no longer see it. `wake`
/// carries workspace ids as [`listen`](crate::listen) hears of them (nil for
/// all); subscribe to it before calling, so no change slips in between.
/// When what they may see changes ([`Rules::guest_access`]), the client is
/// asked to resync.
pub async fn follow<R: Rules, F, Fut>(
    socket: WebSocket,
    pool: &PgPool,
    mut wake: broadcast::Receiver<Uuid>,
    actor: Actor,
    from: Position,
    mut still_allowed: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let (mut sender, mut receiver) = socket.split();
    // Before the catch-up, so the shared reads start where it ends at the latest.
    let tail = Tail::join(pool, &wake, actor.workspace_id);
    tail.prime().await?;

    let epoch = epoch_of::<R>(pool, actor).await?;
    let current = current_sync_id(pool, actor.workspace_id).await?;
    if from.epoch != Some(epoch) || from.after > current {
        send::<R>(&mut sender, &ServerMessage::Resync).await?;
        return Ok(());
    }

    let mut last = from.after;
    if !send_new::<R, _, _>(
        &mut sender,
        pool,
        actor,
        &mut last,
        epoch,
        &mut still_allowed,
    )
    .await?
    {
        return Ok(());
    }

    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.tick().await;
    loop {
        // A heartbeat reads the log again even if no wake says so, in case
        // a notification was missed.
        let reread = tokio::select! {
            woken = wake.recv() => match woken {
                Ok(id) if id == actor.workspace_id || id.is_nil() => false,
                Ok(_) => continue,
                Err(RecvError::Lagged(_)) => false,
                Err(RecvError::Closed) => return Ok(()),
            },
            _ = heartbeat.tick() => {
                sender.send(Message::Ping(Default::default())).await?;
                send::<R>(&mut sender, &ServerMessage::Alive).await?;
                true
            }
            incoming = receiver.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(_)) => continue,
            },
        };
        let Some((current_epoch, rows)) = tail.after(last, reread).await? else {
            // Too far behind for the shared entries: read the log itself.
            if !send_new::<R, _, _>(
                &mut sender,
                pool,
                actor,
                &mut last,
                epoch,
                &mut still_allowed,
            )
            .await?
            {
                return Ok(());
            }
            continue;
        };
        if !send_shared::<R, _, _>(
            &mut sender,
            pool,
            actor,
            &mut last,
            (epoch, current_epoch),
            rows,
            &mut still_allowed,
        )
        .await?
        {
            return Ok(());
        }
    }
}

/// Sends `rows`, the entries after `last` read for every feed of the
/// workspace, page by page, and advances `last`. Before each page, and once
/// even when there is nothing to send, it checks that the viewer may still
/// see the workspace, and as what: the rows were read before, so this is the
/// check that counts. The epochs are the client's and the workspace's now.
async fn send_shared<R: Rules, F, Fut>(
    sender: &mut Sender,
    pool: &PgPool,
    actor: Actor,
    last: &mut i64,
    (epoch, workspace_epoch): (Uuid, Uuid),
    mut rows: Vec<LogRow>,
    still_allowed: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    loop {
        let rest = rows.split_off(rows.len().min(PAGE as usize));
        let page = std::mem::replace(&mut rows, rest);
        if !still_allowed().await {
            sender.send(Message::Close(None)).await?;
            return Ok(false);
        }
        let guest = {
            let mut db = pool.acquire().await?;
            R::guest_access(&mut db, actor).await?
        };
        if served_epoch(workspace_epoch, guest) != epoch {
            send::<R>(sender, &ServerMessage::Resync).await?;
            return Ok(false);
        }
        let Some(upto) = page.last().map(|row| row.sync_id) else {
            return Ok(true);
        };
        let visible = match guest {
            // A guest's view is read with their access, in one snapshot.
            Some(version) if page.iter().any(|row| row.data.is_some()) => {
                let mut db = pool.begin().await?;
                sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
                    .execute(&mut *db)
                    .await?;
                if R::guest_access(&mut db, actor).await? != Some(version) {
                    send::<R>(sender, &ServerMessage::Resync).await?;
                    return Ok(false);
                }
                let visible = guest_sees_of::<R::Model>(&mut db, actor, &page).await?;
                db.commit().await?;
                Some(visible)
            }
            _ => None,
        };
        let entries = visible_entries(page, actor, visible.as_ref())?;
        *last = upto;
        send::<R>(
            sender,
            &ServerMessage::Delta {
                entries,
                sync_id: upto,
            },
        )
        .await?;
        if rows.is_empty() {
            return Ok(true);
        }
    }
}

/// The latest entries of one workspace's log, read once for all the feeds
/// that follow it in this server (with the same pool and wake channel).
///
/// It holds every entry after `base` up to `through`, and is current with
/// every wake it has taken from `wake`: a feed woken by a wake still waiting
/// there gets the log read again first. A wake reaches every receiver at
/// once, so a feed that heard one either finds it waiting here or finds the
/// entries read after it was taken.
struct Tail {
    workspace_id: Uuid,
    pool: PgPool,
    wake: Mutex<broadcast::Receiver<Uuid>>,
    /// `None` before the first read, and after one that failed or was cut
    /// short, which may have taken wakes without reading what they announced.
    window: tokio::sync::Mutex<Option<Window>>,
}

struct Window {
    epoch: Uuid,
    base: i64,
    through: i64,
    rows: VecDeque<LogRow>,
}

/// The tails feeds follow now, by workspace.
static TAILS: LazyLock<Mutex<HashMap<Uuid, Vec<Weak<Tail>>>>> = LazyLock::new(Default::default);

impl Tail {
    /// The tail of `workspace_id` that the feeds woken by `wake` on `pool`
    /// share, made if none follows it yet.
    fn join(pool: &PgPool, wake: &broadcast::Receiver<Uuid>, workspace_id: Uuid) -> Arc<Tail> {
        let mut tails = TAILS.lock().unwrap_or_else(PoisonError::into_inner);
        tails.retain(|_, list| {
            list.retain(|tail| tail.strong_count() > 0);
            !list.is_empty()
        });
        let list = tails.entry(workspace_id).or_default();
        // Two servers in one process (tests, the desktop app's profiles) have
        // a pool and a channel each; the same workspace id in both stays apart.
        let found = list.iter().filter_map(Weak::upgrade).find(|tail| {
            std::ptr::eq(tail.pool.options(), pool.options())
                && tail
                    .wake
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .same_channel(wake)
        });
        found.unwrap_or_else(|| {
            let tail = Arc::new(Tail {
                workspace_id,
                pool: pool.clone(),
                // Only wakes from now on: the first read covers what came before.
                wake: Mutex::new(wake.resubscribe()),
                window: tokio::sync::Mutex::new(None),
            });
            list.push(Arc::downgrade(&tail));
            tail
        })
    }

    /// Reads where the log is now, if nothing has yet.
    async fn prime(&self) -> anyhow::Result<()> {
        let mut window = self.window.lock().await;
        if window.is_none() {
            *window = Some(self.head().await?);
        }
        Ok(())
    }

    /// The workspace's epoch, and the entries after `last`; read again first
    /// if a wake came since the last read, or if `reread`. `None` if the
    /// entries after `last` aren't all here any more.
    async fn after(&self, last: i64, reread: bool) -> anyhow::Result<Option<(Uuid, Vec<LogRow>)>> {
        let mut window = self.window.lock().await;
        let woken = self.take_wakes();
        if reread || woken || window.is_none() {
            // Taken first: a read that fails or is dropped leaves `None`.
            let old = window.take();
            *window = Some(self.read(old).await?);
        }
        let Some(window) = window.as_ref() else {
            unreachable!("just read")
        };
        if last < window.base {
            return Ok(None);
        }
        let rows = window
            .rows
            .iter()
            .filter(|row| row.sync_id > last)
            .cloned()
            .collect();
        Ok(Some((window.epoch, rows)))
    }

    /// Takes the waiting wakes; whether one was for this workspace, for
    /// every workspace, or may have been missed.
    fn take_wakes(&self) -> bool {
        let mut wake = self.wake.lock().unwrap_or_else(PoisonError::into_inner);
        let mut woken = false;
        loop {
            match wake.try_recv() {
                Ok(id) => woken |= id == self.workspace_id || id.is_nil(),
                Err(TryRecvError::Lagged(_)) => woken = true,
                Err(TryRecvError::Closed) => return true,
                Err(TryRecvError::Empty) => return woken,
            }
        }
    }

    /// `old` with the entries logged since, or a fresh start at the head of
    /// the log when there are too many to keep or the history started over.
    async fn read(&self, old: Option<Window>) -> anyhow::Result<Window> {
        let Some(mut window) = old else {
            return self.head().await;
        };
        // The epoch and the entries in one statement, so they agree.
        let rows = sqlx::query(
            "SELECT w.sync_epoch, l.sync_id, l.transaction_id, l.model, l.model_id, l.action, \
                    l.data, l.visible_to \
             FROM workspaces w LEFT JOIN LATERAL ( \
                 SELECT * FROM sync_log \
                 WHERE workspace_id = w.id AND sync_id > $2 ORDER BY sync_id LIMIT $3 \
             ) l ON true \
             WHERE w.id = $1 ORDER BY l.sync_id",
        )
        .bind(self.workspace_id)
        .bind(window.through)
        .bind(KEEP as i64 + 1)
        .fetch_all(&self.pool)
        .await?;
        let Some(first) = rows.first() else {
            return Err(crate::SyncError::NotFound("workspace".into()).into());
        };
        let epoch: Uuid = first.try_get("sync_epoch")?;
        if epoch != window.epoch || rows.len() > KEEP {
            return self.head().await;
        }
        for row in &rows {
            if row.try_get::<Option<i64>, _>("sync_id")?.is_some() {
                let row = LogRow::from_row(row)?;
                window.through = row.sync_id;
                window.rows.push_back(row);
            }
        }
        while window.rows.len() > KEEP {
            if let Some(row) = window.rows.pop_front() {
                window.base = row.sync_id;
            }
        }
        Ok(window)
    }

    /// Nothing kept, at the head of the log: feeds behind it read the log
    /// themselves once.
    async fn head(&self) -> anyhow::Result<Window> {
        let (epoch, head): (Uuid, i64) = sqlx::query_as(
            "SELECT sync_epoch, \
                    (SELECT COALESCE(MAX(sync_id), 0) FROM sync_log WHERE workspace_id = w.id) \
             FROM workspaces w WHERE id = $1",
        )
        .bind(self.workspace_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| crate::SyncError::NotFound("workspace".into()))?;
        Ok(Window {
            epoch,
            base: head,
            through: head,
            rows: VecDeque::new(),
        })
    }
}

/// The epoch `actor`'s copy belongs to now.
async fn epoch_of<R: Rules>(pool: &PgPool, actor: Actor) -> anyhow::Result<Uuid> {
    let epoch = workspace_epoch(pool, actor.workspace_id).await?;
    let mut db = pool.acquire().await?;
    Ok(served_epoch(epoch, R::guest_access(&mut db, actor).await?))
}

type Sender = SplitSink<WebSocket, Message>;

/// Sends every change after `last`, page by page, and advances `last`.
async fn send_new<R: Rules, F, Fut>(
    sender: &mut Sender,
    pool: &PgPool,
    actor: Actor,
    last: &mut i64,
    epoch: Uuid,
    still_allowed: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    loop {
        if !check_access::<R, _, _>(sender, pool, actor, epoch, still_allowed).await? {
            return Ok(false);
        }
        // `upto` can pass entries meant for others; moving the client past
        // them is what confirms its transactions that created them.
        let (entries, upto) = entries_after::<R>(pool, actor, *last, PAGE).await?;
        if upto == *last {
            return Ok(true);
        }
        // A removal or changed grant during the read must discard this page.
        if !check_access::<R, _, _>(sender, pool, actor, epoch, still_allowed).await? {
            return Ok(false);
        }
        *last = upto;
        send::<R>(
            sender,
            &ServerMessage::Delta {
                entries,
                sync_id: upto,
            },
        )
        .await?;
    }
}

async fn check_access<R: Rules, F, Fut>(
    sender: &mut Sender,
    pool: &PgPool,
    actor: Actor,
    epoch: Uuid,
    still_allowed: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    if !still_allowed().await {
        sender.send(Message::Close(None)).await?;
        return Ok(false);
    }
    if epoch_of::<R>(pool, actor).await? != epoch {
        send::<R>(sender, &ServerMessage::Resync).await?;
        return Ok(false);
    }
    Ok(true)
}

async fn send<R: Rules>(
    sender: &mut Sender,
    message: &ServerMessage<R::Model>,
) -> anyhow::Result<()> {
    let text = serde_json::to_string(message)?;
    sender.send(Message::Text(text.into())).await?;
    Ok(())
}
