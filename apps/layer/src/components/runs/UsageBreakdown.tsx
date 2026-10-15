import { memo } from "react";
import { formatDuration, formatTokens, totalTokens, type AgentRow } from "../../lib/runs";
import type { RepoRow } from "../../lib/usage";

const worked = (secs: number, said: boolean) => (said ? formatDuration(secs * 1000) : "—");

/** Per repository, and each agent side by side (RUN-8). */
export const UsageBreakdown = memo(function UsageBreakdown({ repos, agents, agentName }: {
  repos: RepoRow[];
  agents: AgentRow[];
  agentName: (id: string) => string;
}) {
  return (
    <div className="run-breakdown">
      <section>
        <h3>By repository</h3>
        <table className="run-table">
          <thead>
            <tr>
              <th>Repository</th>
              <th>Tasks</th>
              <th>Finished</th>
              <th title="Agents' working time, where they say. A run at a task's root counts for each of its repositories.">Agents working</th>
              <th title="A run at a task's root counts for each of its repositories, so these add up to more than the total">Tokens</th>
            </tr>
          </thead>
          <tbody>
            {repos.map((r) => (
              <tr key={r.id}>
                <td>{r.name}</td>
                <td>{r.tasks}</td>
                <td>{r.finished}</td>
                <td>{worked(r.workedSecs, r.workedSecs > 0)}</td>
                <td>{r.tokens ? formatTokens(r.tokens) : "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <section>
        <h3>By agent</h3>
        <table className="run-table">
          <thead>
            <tr>
              <th>Agent</th>
              <th>Runs</th>
              <th title="Working time, where the agent says">Working</th>
              <th>Tokens</th>
              <th title="Times it stopped on something only you could answer">Asked you</th>
              <th>Failed tools</th>
              <th title="Failed, or a loop gave up">Went wrong</th>
            </tr>
          </thead>
          <tbody>
            {agents.map(({ agent, stats: s }) => (
              <tr key={agent}>
                <td>{agentName(agent)}</td>
                <td>{s.runs}</td>
                <td>{worked(s.workedSecs, s.withWorked > 0)}</td>
                <td>{s.tokens ? formatTokens(totalTokens(s.tokens)) : "—"}</td>
                <td>{s.asks}</td>
                <td>{s.tools?.calls ? `${Math.round((s.tools.failed / s.tools.calls) * 100)}%` : "—"}</td>
                <td className={s.wrong ? "bad" : ""}>{Math.round(s.wrongRate * 100)}%</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
});
