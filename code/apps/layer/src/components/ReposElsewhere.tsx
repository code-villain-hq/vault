import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import type { Elsewhere } from "../lib/types-profiles";
import { BusyLabel, ErrorMessage, useFailure } from "./ui";

/**
 * The workspace's repositories with no clone on this Mac (REPO-9), each
 * with its one next action: Clone, or Locate a clone that is here already.
 */
export function ReposElsewhere({ repos, onPlaced }: { repos: Elsewhere[]; onPlaced: () => void }) {
  if (repos.length === 0) return null;
  return (
    <div className="card">
      {repos.map((r) => <Row key={r.id} repo={r} onPlaced={onPlaced} />)}
    </div>
  );
}

function Row({ repo, onPlaced }: { repo: Elsewhere; onPlaced: () => void }) {
  const [busy, setBusy] = useState<"locate" | "clone" | null>(null);
  const failure = useFailure();

  async function place(how: "locate" | "clone") {
    const chosen = await openDialog({
      directory: true,
      title: how === "locate" ? `Where is ${repo.name}?` : `Clone ${repo.name} into which folder?`,
    });
    if (typeof chosen !== "string") return;
    setBusy(how);
    failure.clear();
    try {
      await (how === "locate" ? api.locateElsewhere(repo.id, chosen) : api.cloneElsewhere(repo.id, chosen));
      onPlaced();
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="repo-manage">
      <div className="repo-line">
        <span className="rname">{repo.name}</span>
        <span className="ui-chip" title="Default branch">{repo.default_branch}</span>
        <span className="ui-status">Not on this Mac</span>
        <div className="spacer" />
        <button className="ui-button" disabled={busy !== null} onClick={() => void place("locate")}>
          Locate…
        </button>
        <button className="ui-button ui-button-primary" disabled={busy !== null} onClick={() => void place("clone")}>
          <BusyLabel busy={busy === "clone"} idle="Clone" working="Cloning…" />
        </button>
      </div>
      <div className="repo-sub">
        <span className="rorigin" title={`Fetches from ${repo.remote_key}`}>
          {repo.remote_key}
          {repo.added_by && <> · added by {repo.added_by}</>}
        </span>
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
    </div>
  );
}
