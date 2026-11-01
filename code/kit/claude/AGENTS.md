# Claude: notes for coding agents

`code-villain-claude` is Claude Code on the asker's Mac, for every studio
app: what a person asks Claude in the desktop app is answered first by
Claude Code on their Mac, wherever the workspace is
(`studio/design/behaviour.md`, "Claude"). The server's key answers only
when Claude Code can't, in the browser, and for work nobody asks for.
Forge uses it. Almanac, the reference, still has its own copy
(`crates/server/src/acp.rs`) and can move onto it. Glyph asks Claude
nothing: it translates with local models only.

## Layout

- `src/acp.rs`: `Adapter`. Claude Code (or ChatGPT) on this Mac, started
  per question over the Agent Client Protocol, in an empty folder of its
  own, as you're signed in to it: no settings, no tools of its own, only the
  app's (`mcp__<app>__<tool>`) and the web's when the question allows it.
  A `Failure` says why it gave no answer: not installed, signed out, or
  what went wrong.
- `src/questions.rs`: `Questions<T>`. The token each question opens, which
  Claude Code sends with every call to the app's tools, and which goes when
  the question is over (`Open` is dropped).
- `src/mcp.rs`: the app's tools for one question, as Claude Code calls them
  (`Tools`, `reply`): one JSON-RPC message per POST, no stream.
- `src/handed.rs`: a server's question handed to the asker's Mac
  (`HandedQuestion`), what the Mac made of it (`Answered`), and the
  server's wait for it.
- `src/here.rs`: `Here`, the desktop app's half, which answers what a
  server hands it.
- `src/info.rs`: whether Claude Code can answer, as Settings → Claude shows
  it.

## Using it in an app

- The server: an endpoint for the app's tools (`POST /api/ai/mcp` in
  Forge) that finds the question by its bearer token and hands each message
  to `mcp::reply`; and `POST /api/answered`, which hands what the Mac said
  to `Questions::answered`. Both answer 401 to a token that's over.
- The desktop app's own server: `Adapter::new(app, Kind::ClaudeCode,
  folder, mcp_url)` with the server's port, and the server asks it itself.
- A server's workspace: the desktop app asks with `here`, the server opens
  the question with `Questions::hand` and replies with
  `handed::question`, then waits with `handed::wait`. Not installed or
  signed out, it gives the question to its key when it has one.
- The desktop shell manages a `Here` and offers two commands,
  `claude_here` and `answer_here`; `code/kit/web`'s `here.ts` calls them
  and keeps the per-Mac setting ("Answer with Claude Code on this Mac").
- Docker builds need `kit/claude` copied, as the server depends on it.

## Changing it

`cargo test` here. CI runs it with the other kits
(`code/kit/dev-id/check-kits.ts`), and `bun run check` in each app that
uses it.
