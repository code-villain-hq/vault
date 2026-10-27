import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { useEpicViews } from "../../lib/epicState";
import { useStore } from "../../store";
import type { SubTaskFailure, TaskView } from "../../lib/types";
import { ErrorMessage, Field, Modal, useFailure } from "../ui";
import { IssueTypeIcon, typeMap } from "../IssueType";
import { creatableTypes, preferredCreatable } from "../task-forms/creatable";
import { RequiredFieldPickers, useRequiredFields } from "../tickets/RequiredFields";

/**
 * File the sub-tasks not filed yet (EPIC-7): asks once for what every new
 * ticket needs, the type and the project's required fields, then files each
 * under the epic. What failed is named, and filing again makes only that.
 */
export function FileSubTasks({ epic, count, onClose }: { epic: TaskView; count: number; onClose: () => void }) {
  const issueTypes = useStore((s) => s.issueTypes);
  const refreshTasks = useStore((s) => s.refreshTasks);
  const refreshIssues = useStore((s) => s.refreshIssues);
  const put = useEpicViews((s) => s.put);
  const failure = useFailure();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<SubTaskFailure[]>([]);

  // With no ticket there is nothing to file in Jira: only tasks are made.
  const project = epic.issue_key?.split("-")[0] ?? "";
  const types = useMemo(() => creatableTypes(issueTypes), [issueTypes]);
  const [typeName, setTypeName] = useState("");
  useEffect(() => {
    if (!typeName || !types.some((t) => t.name === typeName)) setTypeName(preferredCreatable(types)?.name ?? "");
  }, [types, typeName]);
  const typeId = types.find((t) => t.name === typeName)?.id;
  // Filing fills these itself, from the spec and the epic.
  const rf = useRequiredFields(project, project ? typeId : undefined, [
    "summary", "description", "issuetype", "project", "parent", "reporter",
  ]);

  async function file() {
    if (busy) return;
    setBusy(true);
    failure.clear();
    try {
      const filed = await api.fileSubTasks(epic.id, project ? typeName || null : null, project ? rf.fields() : null);
      put(epic.id, filed.epic);
      await Promise.all([refreshTasks(), refreshIssues()]);
      if (filed.failed.length === 0) onClose();
      setFailed(filed.failed);
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  const blocked = busy || (!!project && (!typeName || rf.missing.length > 0));
  return (
    <Modal
      title={`File ${count} sub-task${count === 1 ? "" : "s"}`}
      onClose={onClose}
      footer={
        <>
          <button className="ui-button" onClick={onClose}>{failed.length > 0 ? "Close" : "Cancel"}</button>
          <button
            className="ui-button ui-button-primary"
            disabled={blocked}
            title={rf.missing.length > 0 ? `${rf.missing.map((f) => f.name).join(", ")} required by this project` : undefined}
            onClick={() => void file()}
          >
            {busy ? "Filing…" : failed.length > 0 ? "File the rest" : "File"}
          </button>
        </>
      }
    >
      <div className="muted" style={{ marginBottom: 12, lineHeight: 1.6 }}>
        {project
          ? `Each becomes a ticket under ${epic.issue_key}, and a task here with its share of the spec, approved. No agent starts and no ticket moves.`
          : "Each becomes a task here with its share of the spec, approved. No agent starts."}
      </div>
      {project && (
        <>
          <Field label="Type">
            <div className="type-row">
              {types.map((t) => (
                <button key={t.id} className={`ui-chip type-pick${typeName === t.name ? " active" : ""}`} onClick={() => setTypeName(t.name)}>
                  <IssueTypeIcon types={typeMap(issueTypes)} name={t.name} size={15} />
                  {t.name}
                </button>
              ))}
            </div>
          </Field>
          <RequiredFieldPickers rf={rf} refused="the sub-tasks — file them by hand and name each key in its heading" />
        </>
      )}
      {failed.length > 0 && (
        <div className="confirm-detail" style={{ marginBottom: 12 }}>
          Not filed:
          <ul style={{ margin: "6px 0 0", paddingLeft: 18 }}>
            {failed.map((f) => <li key={f.title}><b>{f.title}</b>: {f.error}</li>)}
          </ul>
        </div>
      )}
      <ErrorMessage>{failure.error}</ErrorMessage>
    </Modal>
  );
}
