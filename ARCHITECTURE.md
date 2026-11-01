# Code Villain architecture

The vault holds the studio's shared direction and the products that implement it.
Forge, Glyph, Raven and Layer share libraries for sync, identity provisioning,
settings and desktop startup. Each app runs its own server and database.

## Repository

```mermaid
flowchart TB
    subgraph Studio["studio/ — shared direction"]
        Brand["brand/<br/>Identity and voice"]
        Design["design/<br/>Principles, behavior and tokens"]
        Content["content/<br/>Blog and social posts"]
        Brand --> Design
    end

    subgraph Code["code/ — implementation"]
        Apps["Forge · Glyph · Raven · Layer<br/>Rust servers, React clients, Tauri desktop"]
        Kit["kit/<br/>Sync · Desktop runtime · SCIM<br/>Settings · Shared web code · Dev identity"]
        Caster["Caster<br/>Native Swift / AppKit"]
        Theme["Code Villain theme<br/>Editors and terminals"]
        Site["Site<br/>Static HTML / CSS, Python build"]

        Apps -->|reuse libraries| Kit
    end

    Design -.-> Apps
    Design -.-> Caster
    Design -.-> Theme
    Design -.-> Site
    Content -->|render blog| Site
```

Brand decisions become [design principles and tokens](studio/design/README.md),
which each app implements in its own toolkit. Caster, the theme and the site have
separate builds from the shared server-and-sync platform.

## Application runtime

This pattern repeats per app. Shared kit code is built into each app; each
company deployment has its own server and database. A client's sync connection
targets the server that owns the selected workspace.

```mermaid
flowchart TB
    subgraph Clients["Clients"]
        Web["React web app"]
        Desktop["Tauri desktop app"]
        Sync["TypeScript sync client"]
        Cache[("IndexedDB<br/>Workspace copy + pending edits")]

        Web --> Sync
        Desktop --> Sync
        Sync <--> Cache
    end

    subgraph Company["Company deployment"]
        Server["Rust / Axum server<br/>App models, permissions and APIs"]
        Engine["Shared Rust sync engine<br/>Snapshots, transactions and change feed"]
        DB[("PostgreSQL<br/>App records, membership, sessions, sync log")]
        Settings["App TOML + environment"]

        Settings --> Server
        Server <--> Engine
        Engine <--> DB
        Server <--> DB
    end

    Identity["Company identity provider<br/>Pocket ID or compatible provider"]
    Identity <-->|OIDC sign-in| Server
    Identity -->|SCIM people and groups| Server

    Sync <-->|HTTP transactions + WebSocket updates| Server

    subgraph Mac["Desktop-owned workspaces on this Mac"]
        Starter["Shared desktop runtime"]
        Local["Local app server<br/>Loopback, no sign-in"]
        LocalDB[("Bundled PostgreSQL 18")]

        Starter -->|starts and stops| Local
        Starter -->|starts and stops| LocalDB
        Local <--> LocalDB
    end

    Desktop -->|owns lifecycle| Starter
    Sync <-->|local workspace connection| Local
```

Each client keeps a local workspace copy and an outbox of pending edits. The
[sync engine](code/kit/sync/AGENTS.md) applies transactions under the app's rules
and streams authorized changes back to clients. The local app server uses the
same engine; it is condensed into one node above.

[Sync engine architecture](code/kit/sync/architecture.md) follows an edit from
the optimistic local view through server validation and back to every authorized
client, including persistence, confirmation and recovery.

Company sign-in uses OpenID Connect. [SCIM](code/kit/scim/AGENTS.md) provisions
people and groups, which determine workspace membership and roles. Desktop-owned
workspaces start their server and database through the
[desktop runtime](code/kit/desktop/AGENTS.md), without company sign-in.

## Layer's local state

Layer keeps tasks, worktrees, terminals, agent runs and secrets on the Mac. Only
shared setup uses workspace sync. Backend edits pass through a durable file
queue into the sync client; the file queue is acknowledged only after IndexedDB
saves the edits. Stable transaction IDs allow interrupted handoffs to retry.

A Layer profile selects either a company server or its own local server. See
[Layer's architecture](code/apps/layer/docs/architecture.md) for its processes,
storage, integrations and queue handoff.
