import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { useStore } from "../../store";
import type { TaskSpec, TaskView } from "../../lib/types";
import { Confirm } from "../ui";

/**
 * Start agent, once for the task above its files (SPEC-12), saying what it
 * starts and how much is left. In a repository's panel, beside its tasks,
 * the button read as starting that repository only, though it started
 * every one; not sure, the user clicked it again in the other panel, and
 * two agents set off on the same steps. So it names the agents already at
 * work in the task, and asks before starting another.
 */
export function SpecStartAgent({ task, spec, agentId, setAgentId }: {
  task: TaskView;
  spec: TaskSpec;
  agentId: string | null;
  setAgentId: (id: string) => void;
}) {
  const agents = useStore((s) => s.agents);
  const panes = useStore((s) => s.panes);
  const refreshPanes = useStore((s) => s.refreshPanes);
  const showPane = useStore((s) => s.showPane);
  const setTab = useStore((s) => s.setTab);
  const fail = useStore((s) => s.fail);
  const [scope, setScope] = useState<string>("all");
  const [starting, setStarting] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const installed = agents.filter((a) => a.installed);
  const firstInstalled = installed[0]?.id;
  useEffect(() => {
    if (!agentId && firstInstalled) setAgentId(firstInstalled);
  }, [agentId, firstInstalled, setAgentId]);

  const repos = spec.repos;
  const one = scope === "all" ? null : repos.find((r) => r.checkout_id === scope) ?? null;
  const inScope = one ? [one] : repos;
  const total = inScope.reduce((n, r) => n + r.steps.length, 0);
  const left = inScope.reduce((n, r) => n + r.steps.filter((s) => !s.done).length, 0);
  const name = installed.find((a) => a.id === agentId)?.name ?? "an agent";
  const where = one ? one.folder : repos.length === 1 ? repos[0].folder : repos.length === 2 ? "both repositories" : `all ${repos.length} repositories`;
  const working = panes.filter((p) => p.task_id === task.id && p.running && p.kind === "agent");
  const nameOf = (agent: string | null) => agents.find((a) => a.id === agent)?.name ?? "An agent";

  async function start() {
    if (!agentId) return;
    setStarting(true);
    try {
      const checkoutId = one && repos.length > 1 ? one.checkout_id : null;
      const [prompt, work] = await Promise.all([api.taskPrompt(task.id, checkoutId), api.specWorkPrompt(task.id, checkoutId)]);
      const pane = await api.spawnAgent(task.id, agentId, checkoutId, [prompt, work].filter(Boolean).join("\n\n"));
      await refreshPanes();
      showPane(pane.id);
      setTab("terminals");
    } catch (e) {
      fail(e);
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="spec-start-bar">
      <select value={agentId ?? ""} disabled={installed.length === 0} onChange={(e) => setAgentId(e.target.value)} aria-label="Agent">
        {installed.length === 0 && <option value="">No agent CLI installed</option>}
        {installed.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
      </select>
      {repos.length > 1 && (
        <select value={scope} onChange={(e) => setScope(e.target.value)} aria-label="Which repositories">
          <option value="all">Every repository, from the task folder</option>
          {repos.map((r) => <option key={r.checkout_id} value={r.checkout_id}>Only {r.folder}</option>)}
        </select>
      )}
      <button
        className="btn btn-sm btn-primary"
        disabled={!agentId || total === 0 || left === 0 || starting}
        title={total === 0 ? "Approve the tasks first" : left === 0 ? "Every step is done" : "It works through the approved steps in order, ticking each off as it finishes it"}
        onClick={() => (working.length > 0 ? setConfirming(true) : void start())}
      >
        {starting ? "Starting…" : total === 0 ? "Start agent" : `Start ${name} on ${where} · ${left} step${left === 1 ? "" : "s"} left`}
      </button>
      <span className="spacer" />
      {working.length > 0 && (
        <span className="spec-working">
          {working.length === 1 ? `${nameOf(working[0].agent_id)} is working in this task` : `${working.length} agents are working in this task`}
          <button className="btn btn-sm" onClick={() => { showPane(working[0].id); setTab("terminals"); }}>Open</button>
        </span>
      )}
      {confirming && (
        <Confirm
          title="Start another agent?"
          body={`${working.length === 1 ? `${nameOf(working[0].agent_id)} is` : `${working.length} agents are`} already working in this task. A second agent on the same steps does them again, alongside the first.`}
          confirmLabel="Start another"
          danger={false}
          onConfirm={() => { setConfirming(false); void start(); }}
          onCancel={() => setConfirming(false)}
        />
      )}
    </div>
  );
}
