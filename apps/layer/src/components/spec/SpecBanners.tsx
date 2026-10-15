import { partView, slotKey, useSpecs, type SpecFile } from "../../lib/specs";
import { useStore } from "../../store";
import type { TaskSpec, TaskView } from "../../lib/types";

/**
 * What needs the user before the file in the editor can be trusted: specs
 * from before to merge (SPEC-25), copies changed outside the app (SPEC-22),
 * and a change an agent proposed (SPEC-24).
 */
export function SpecBanners({ task, spec, file, busy, onApprove, onShow }: {
  task: TaskView;
  spec: TaskSpec;
  file: SpecFile;
  busy: boolean;
  onApprove: (files: SpecFile[]) => void;
  /** Put a copy's text into the editor, to read it there. */
  onShow: (text: string, checkoutId: string) => void;
}) {
  const merge = useSpecs((s) => s.merge);
  const approve = useSpecs((s) => s.approve);
  const putBack = useSpecs((s) => s.putBack);
  const turnDown = useSpecs((s) => s.turnDown);
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);
  const view = partView(spec, file.part, file.checkoutId);
  const word = file.part === "tasks" ? "tasks" : file.part === "design" ? "design" : spec.kind === "bugfix" ? "bug analysis" : "requirements";

  return (
    <>
      {spec.split && (
        <div className="spec-banner">
          <span>
            Each repository has a spec of its own from before, and they differ. Merge them into one:
            requirements and a design for the whole task, and each repository's tasks with their ticks
            kept. Nothing is written until you approve it.
          </span>
          <button className="btn btn-sm" disabled={busy} onClick={() => void merge(task.id).catch(fail)}>Merge into one</button>
        </div>
      )}
      {view?.outside.map((o) => (
        <div key={o.checkout_id} className="spec-banner warn">
          <span>
            In <b>{o.folder}</b>, the {word} {o.text ? "were changed" : "were deleted"} outside the app, not
            approved by you. Agents and checks still go by what was approved.
          </span>
          {o.text && (
            <>
              <button className="btn btn-sm" disabled={busy} title="Put its text in the editor, to read and approve or discard" onClick={() => onShow(o.text, o.checkout_id)}>Show it</button>
              <button
                className="btn btn-sm"
                disabled={busy}
                title={file.part === "tasks" ? "Approve it as these tasks" : "Approve it, and write it into every repository"}
                onClick={() => {
                  const target: SpecFile = { part: file.part, checkoutId: file.part === "tasks" ? o.checkout_id : null };
                  void approve(task.id, [target], null, { [slotKey(target.part, target.checkoutId)]: o.text })
                    .then((failed) => toast(failed.length ? "error" : "success", failed.length ? `Not written in ${failed.map((f) => f.folder).join(", ")}.` : `Approved the ${word} from ${o.folder}.`))
                    .catch(fail);
                }}
              >
                Approve it
              </button>
            </>
          )}
          <button
            className="btn btn-sm"
            disabled={busy}
            title="Write what was approved back over it, and commit that"
            onClick={() => void putBack(task.id, file.part, o.checkout_id).then(() => toast("success", `Put back the approved ${word} in ${o.folder}.`)).catch(fail)}
          >
            Put back
          </button>
        </div>
      ))}
      {view?.proposal && (
        <div className="spec-banner">
          <span>
            <b>{view.proposal.by}</b> proposes this change: {view.proposal.why}
          </span>
          <button className="btn btn-sm btn-primary" disabled={busy} onClick={() => onApprove([file])}>Approve it</button>
          <button
            className="btn btn-sm"
            disabled={busy}
            title="Drop the change, and tell the agent"
            onClick={() => void turnDown(task.id, file).catch(fail)}
          >
            Turn down
          </button>
        </div>
      )}
    </>
  );
}
