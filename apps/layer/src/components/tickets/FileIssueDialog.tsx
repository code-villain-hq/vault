import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { read, write } from "../../lib/persist";
import { useStore } from "../../store";
import type { JiraIssue, JiraIssueType } from "../../lib/types";
import { Combo, Field, Modal } from "../ui";
import { IssueTypeIcon, type TypeMap } from "../IssueType";
import { creatableTypes, preferredCreatable } from "../task-forms/creatable";
import { OptimizeDescription } from "./OptimizeDescription";
import { RequiredFieldPickers, useRequiredFields } from "./RequiredFields";

export function FileIssueDialog({
  epic,
  issueTypes,
  types,
  onClose,
  onFiled,
}: {
  /** Set when filing under an epic. Absent means a ticket in a project you pick. */
  epic: { key: string; summary: string } | null;
  issueTypes: JiraIssueType[];
  types: TypeMap;
  onClose: () => void;
  onFiled: (issue: JiraIssue) => void;
}) {
  const settings = useStore((s) => s.settings);
  const issues = useStore((s) => s.issues);
  const refreshIssues = useStore((s) => s.refreshIssues);
  const toast = useStore((s) => s.toast);
  const fail = useStore((s) => s.fail);

  const [project, setProject] = useState(() => read("jiraProject", ""));
  const [newSummary, setNewSummary] = useState("");
  const [newDesc, setNewDesc] = useState("");
  const [newType, setNewType] = useState("");
  const [filingBusy, setFilingBusy] = useState(false);

  const creatable = useMemo(() => creatableTypes(issueTypes), [issueTypes]);
  const boardKeys = useMemo(
    () => [...new Set(issues.map((i) => i.key.split("-")[0]).filter(Boolean))],
    [issues],
  );
  const defaultProject = settings?.jira?.project_key ?? boardKeys[0] ?? "";
  // An epic already says which project. Otherwise the last one filed in, then
  // the board's own key.
  const projectKey = epic ? epic.key.split("-")[0] : project.trim();

  useEffect(() => {
    if (epic) return;
    setProject((current) => current || defaultProject);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (newType && creatable.some((t) => t.name === newType)) return;
    setNewType(preferredCreatable(creatable)?.name ?? "");
  }, [creatable, newType]);

  const typeId = creatable.find((t) => t.name === newType)?.id;
  // Fields the dialog fills itself. Parent is one of them only when an epic
  // was named — a ticket filed on its own does not have one, and a project
  // that insists on a parent should say so.
  const rf = useRequiredFields(projectKey, typeId, [
    "summary", "description", "issuetype", "project", "reporter",
    ...(epic ? ["parent"] : []),
  ]);
  const descriptionRequired = rf.descriptionRequired;

  const missing = [
    ...rf.missing,
    ...(descriptionRequired && !newDesc.trim()
      ? [{ id: "description", name: "Description" }]
      : []),
  ];

  async function fileIssue() {
    // The same conditions as the button. Enter in the summary called this
    // directly, so a second Enter — or key repeat — filed a second ticket.
    if (filingBusy || !newSummary.trim() || !newType || !projectKey || missing.length > 0) return;
    setFilingBusy(true);
    try {
      const issue = await api.jiraCreateIssue({
        summary: newSummary.trim(),
        description: newDesc.trim(),
        issue_type: newType,
        project_key: epic ? null : projectKey,
        parent_key: epic?.key ?? null,
        fields: rf.fields(),
      });
      toast("success", epic ? `Filed ${issue.key} under ${epic.key}` : `Filed ${issue.key}`);
      onClose();
      await refreshIssues();
      // Straight into the start-work dialog: filing it is usually the first
      // half of picking it up.
      onFiled(issue);
    } catch (e) {
      fail(e);
    } finally {
      setFilingBusy(false);
    }
  }

  return (
    <Modal
      title={epic ? `New ticket in ${epic.key}` : "New ticket"}
      onClose={onClose}
      footer={
        <>
          <button className="ui-button" onClick={onClose}>Cancel</button>
          <button
            className="ui-button ui-button-primary"
            disabled={filingBusy || !newSummary.trim() || !newType || !projectKey || missing.length > 0}
            title={
              missing.length > 0
                ? `${missing.map((f) => f.name).join(", ")} required by this project`
                : undefined
            }
            onClick={() => void fileIssue()}
          >
            {filingBusy ? "Filing…" : "File ticket"}
          </button>
        </>
      }
    >
      <div className="muted" style={{ marginBottom: 12, lineHeight: 1.6 }}>
        {epic ? (
          <>
            Filed under <b>{epic.key}</b>
            {epic.summary ? ` — ${epic.summary}` : ""}, in that epic's own project.
          </>
        ) : (
          <>Filed in the project you pick, with no parent epic.</>
        )}{" "}
        Nothing is checked out; the start-work dialog opens once it exists.
      </div>

      {!epic && (
        <Field label="Project" hint="The key the ticket is filed under.">
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
      )}

      <Field label="Summary">
        <input className="ui-field"
          autoFocus
          value={newSummary}
          onChange={(e) => setNewSummary(e.target.value)}
          placeholder="What needs doing"
          onKeyDown={(e) => { if (e.key === "Enter") void fileIssue(); }}
        />
      </Field>

      <Field label="Type">
        <div className="type-row">
          {creatable.map((t) => (
            <button
              key={t.id}
              className={`ui-chip type-pick${newType === t.name ? " active" : ""}`}
              onClick={() => setNewType(t.name)}
            >
              <IssueTypeIcon types={types} name={t.name} size={15} />
              {t.name}
            </button>
          ))}
        </div>
      </Field>

      <RequiredFieldPickers rf={rf} refused="the ticket — file it" />

      <Field
        label="Description"
        hint={
          descriptionRequired
            ? "Required by this project. Becomes the ticket body and an agent's briefing."
            : "Optional. Becomes the ticket body and an agent's briefing."
        }
      >
        <textarea className="ui-field"
          rows={5}
          value={newDesc}
          onChange={(e) => setNewDesc(e.target.value)}
          placeholder="What needs doing, and how you would know it is done."
        />
        <OptimizeDescription
          summary={newSummary}
          description={newDesc}
          kind="ticket"
          onChange={setNewDesc}
          disabled={filingBusy}
        />
      </Field>
    </Modal>
  );
}
