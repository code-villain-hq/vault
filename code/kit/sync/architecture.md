# Sync engine architecture

Forge, Glyph, Raven and Layer use the same engine. Each app supplies its models,
validation and permission rules; each workspace syncs with the server that owns
it, whether that server runs for a company or on the desktop's loopback address.

## One edit from local view to other clients

```mermaid
sequenceDiagram
    actor Person
    participant C as Sync client
    participant I as IndexedDB
    participant S as App server + sync engine
    participant P as PostgreSQL
    participant O as Other clients

    Note over C,I: Startup restores confirmed records, pending transactions, epoch and cursor
    Note over C,S: First use downloads a snapshot; reconnect resumes the feed from the saved cursor

    Person->>C: Edit a record
    C->>C: Give transaction a stable ID; add to pending edits
    C-->>Person: Show confirmed records + pending edits immediately
    par Save locally
        C->>I: Queue outbox persistence
        I-->>C: Saved, or storage error for retry
    and Send when possible
        C->>S: POST transaction with the same ID on retries
        S->>P: Begin transaction; lock workspace; check duplicate ID
        Note over S,P: For a new transaction: check app rules and permissions
        S->>P: Atomically write records, sync log and transaction result
        P-->>S: Commit; NOTIFY wakes change feeds
        S-->>C: Accepted: last sync_id, or null for no changes
    end

    Note over S,O: The workspace's feeds read the change once; each checks its viewer's access, then sends what they may see
    S-->>C: WebSocket delta with records and cursor
    S-->>O: WebSocket delta for each authorized viewer
    C->>C: Update confirmed records; advance cursor
    C->>C: Remove pending edits whose accepted sync_id is reached
    C->>C: Replay remaining pending edits over confirmed records
    C->>I: Queue records, cursor and outbox updates in order
    C-->>Person: Show the reconciled view
    O->>O: Apply delta and replay its own pending edits
```

Local persistence and network delivery run independently. An edit appears before
either finishes. The client reports storage errors and retains failed writes in
memory for retry; only a completed local save protects those edits across a
process exit. Callers that transfer ownership of edits, such as Layer's backend
queue, wait for `saved()` before acknowledging that handoff.

The POST response and WebSocket delta may arrive in either order. A transaction
is removed from the outbox when its response says it changed nothing, or when
the client's cursor has reached the accepted transaction's last `sync_id`.
The server stores the result by transaction ID, so sending the same transaction
again does not apply its operations again.

## What the client keeps

| State | Purpose |
| --- | --- |
| Confirmed records | The last server data received through a snapshot or delta |
| Outbox | Transactions still pending confirmation, in local order |
| Visible records | Confirmed records with pending operations replayed over them |
| `sync_id` | How far this client has read through the workspace's change log |
| `epoch` | Which version of the workspace history and access scope the copy belongs to |

IndexedDB stores confirmed records, the outbox, cursor and epoch. The visible
view is rebuilt in memory. Incoming data therefore does not erase an edit that
is still pending locally.

## Recovery and rejection

- **First use:** bootstrap returns a consistent snapshot of authorized records,
  its `sync_id` and its epoch. The WebSocket follows changes after that position.
- **Offline or temporary server failure:** keep pending edits and retry with the
  same transaction IDs. Reconnect the feed from the last received cursor.
- **Permanent refusal:** remove the refused transaction, rebuild its affected
  records from confirmed data and remaining edits, and report the error through
  the app's rejection callback.
- **Changed history or access scope:** the server requests `resync`; the client
  downloads a new snapshot, preserves pending transactions and replays them.
  When the epoch changes, pending transactions are marked for sending again.
- **Revoked access:** the feed closes or requests resync as appropriate. Reads
  and writes still require authorization; pending edits do not confer access.
- **Unread local storage:** retry loading before opening the server connection,
  so a fresh snapshot cannot overwrite an unread outbox.

The sync log is the durable source for catch-up. PostgreSQL NOTIFY wakes feeds;
it does not carry the records. A heartbeat also checks for missed changes.
The feeds following one workspace in a server share their reads of the log's
latest entries, so a change is read once, not once per viewer. Each feed still
checks its own viewer before every page it sends and on every wake: the app's
check (signed in, still a member) and their access (role, guest version). A
feed that falls behind the shared entries, or is catching up after
connecting, reads the log itself.
Workspace write locking keeps sync IDs in commit order, and each transaction's
operations succeed or fail together. This is server-ordered transaction sync,
not a CRDT or peer-to-peer merge protocol.

## Code map

| Responsibility | Source |
| --- | --- |
| Client state, optimistic replay, confirmation and retry | [client.ts](client/src/client.ts) |
| IndexedDB persistence | [indexeddb.ts](client/src/indexeddb.ts) |
| HTTP writes and WebSocket transport | [transport.ts](client/src/transport.ts) |
| Transaction application and app rule hooks | [apply.rs](src/apply.rs) |
| Authorized snapshots and log reads | [read.rs](src/read.rs) |
| Catch-up, live feed, shared log reads and access checks | [feed.rs](src/feed.rs) |
| Feed behaviour and query count with several viewers | [tests/feed.rs](tests/feed.rs) |
