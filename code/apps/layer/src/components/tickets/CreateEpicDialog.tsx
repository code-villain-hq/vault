import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { read, write } from "../../lib/persist";
import { useStore } from "../../store";
import type { JiraIssue, JiraIssueType } from "../../lib/types";
import { Combo, ErrorMessage, Field, Modal, useFailure } from "../ui";
import { IssueTypeIcon, type TypeMap } from "../IssueType";
import { epicTypes, preferredEpic } from "../task-forms/creatable";
import { OptimizeDescription } from "./OptimizeDescription";
import { RequiredFieldPickers, useRequiredFields } from "./RequiredFields";

export function CreateEpicDialog({
  issueTypes,
  types,
  issues,
  onClose,
  onCreated,
}: {
  issueTypes: JiraIssueType[];
  types: TypeMap;
  issues: JiraIssue[];
  onClose: () => void;
  /** Offers Start work on it, to write its spec (EPIC-3). */
  onCreated: (epic: JiraIssue) => void;
}) {
  const settings = useStore((s) => s.settings);
  const refreshIssues = useStore((s) => s.refreshIssues);
  // Creating fails in the dialog, above its buttons (NOTE-6).
  const failure = useFailure();

  const [summary, setSummary] = useState("");
  const [desc, setDesc] = useState("");
  const [epicType, setEpicType] = useState("");
  const [project, setProject] = useState(() => read("jiraProject", ""));
  const [busy, setBusy] = useState(false);

  const epics = useMemo(() => epicTypes(issueTypes), [issueTypes]);
  const boardKeys = useMemo(
    () => [...new Set(issues.map((i) => i.key.split("-")[0]).filter(Boolean))],
    [issues],
  );
  const defaultProject = settings?.jira?.project_key ?? boardKeys[0] ?? "";

  useEffect(() => {
    setProject((current) => current || defaultProject);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (epicType && epics.some((t) => t.name === epicType)) return;
    setEpicType(preferredEpic(epics)?.name ?? "");
  }, [epics, epicType]);

  const typeId = epics.find((t) => t.name === epicType)?.id;
  // Fields the dialog fills itself, whether or not Jira calls them required.
  const rf = useRequiredFields(project.trim(), typeId, [
    "summary", "description", "issuetype", "project", "parent", "reporter",
  ]);
  const descriptionRequired = rf.descriptionRequired;

  const missing = [
    ...rf.missing,
    ...(descriptionRequired && !desc.trim()
      ? [{ id: "description", name: "Description" }]
      : []),
  ];

  async function createEpic() {
    // The button's own conditions: Enter in the summary came straight here,
    // and a second Enter created a second epic.
    if (busy || !summary.trim() || !epicType || !project.trim() || missing.length > 0) return;
    setBusy(true);
    failure.clear();
    try {
      // The epic appears in Tickets (NOTE-7).
      const epic = await api.jiraCreateIssue({
        summary: summary.trim(),
        description: desc.trim(),
        issue_type: epicType,
        project_key: project.trim(),
        parent_key: null,
        fields: rf.fields(),
      });
      onClose();
      await refreshIssues();
      onCreated(epic);
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title="New epic"
      onClose={onClose}
      footer={
        <>
          <button className="ui-button" onClick={onClose}>Cancel</button>
          <button
            className="ui-button ui-button-primary"
            disabled={
              busy ||
              !summary.trim() ||
              !epicType ||
              !project.trim() ||
              missing.length > 0
            }
            title={
              missing.length > 0
                ? `${missing.map((f) => f.name).join(", ")} required by this project`
                : undefined
            }
            onClick={() => void createEpic()}
          >
            {busy ? "Creating…" : "Create epic"}
          </button>
        </>
      }
    >
      <div className="muted" style={{ marginBottom: 12, lineHeight: 1.6 }}>
        Creates a top-level epic in the project you pick. Then pick its
        repositories to write its spec: its tasks become its sub-tasks.
      </div>

      <Field label="Project" hint="The key the epic is filed under.">
        <Combo
          value={project}
          options={boardKeys}
          placeholder="ACME"
          width="100%"
          onChange={(raw) => {
            const v = raw.trim().toUpperCase();
            setProject(v);
            write("jiraProject", v);
          }}
        />
      </Field>

      <Field label="Summary">
        <input className="ui-field"
          autoFocus
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
          placeholder="What this epic covers"
          onKeyDown={(e) => { if (e.key === "Enter") void createEpic(); }}
        />
      </Field>

      <Field label="Type">
        <div className="type-row">
          {epics.map((t) => (
            <button
              key={t.id}
              className={`ui-chip type-pick${epicType === t.name ? " active" : ""}`}
              onClick={() => setEpicType(t.name)}
            >
              <IssueTypeIcon types={types} name={t.name} size={15} />
              {t.name}
            </button>
          ))}
        </div>
      </Field>

      <RequiredFieldPickers rf={rf} refused="the epic — create it" />

      <Field
        label="Description"
        hint={
          descriptionRequired
            ? "Required by this project."
            : "Optional. Becomes the epic body."
        }
      >
        <textarea className="ui-field"
          rows={5}
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
          placeholder="Scope, outcomes, and anything an agent should know."
        />
        <OptimizeDescription
          summary={summary}
          description={desc}
          kind="epic"
          onChange={setDesc}
          disabled={busy}
        />
      </Field>
      <ErrorMessage>{failure.error}</ErrorMessage>
    </Modal>
  );
}
