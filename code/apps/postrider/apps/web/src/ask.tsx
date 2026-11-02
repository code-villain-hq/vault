// The field at the foot of the Inbox, a folder and a search: ask Postrider
// something, or tell it what to do. Claude Code on this Mac answers
// (`ask.rs`). The answer shows above the field; what it changed, such as a
// rule it made and the mail it moved, comes with Undo and the folder to open.

import type { Answer, Asked } from "@postrider/types";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { ArrowUp, Folder, Sparkles, X } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { api, messageOf } from "./api";
import { useFolders } from "./mail";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

type Said = {
  question: string;
  /** None while Postrider is thinking. */
  answer?: Answer;
  problem?: string;
};

export function Ask() {
  const { workspace, place } = useWorkspace();
  const { moved } = useFolders();
  const [text, setText] = useState("");
  const [said, setSaid] = useState<Said | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const thinking = said !== null && !said.answer && !said.problem;

  const send = () => {
    const question = text.trim();
    // One question at a time: the next waits for this answer.
    if (!question || thinking) return;
    setText("");
    setSaid({ question });
    const asked: Asked = { text: question };
    api<Answer>("POST", `/api/w/${workspace.id}/ask`, asked, place).then(
      (answer) => {
        setSaid({ question, answer });
        // A rule it made may have made a folder and moved mail into it.
        if (answer.id) moved();
      },
      (error) => setSaid({ question, problem: messageOf(error) }),
    );
  };

  return (
    <div className="ask flex max-h-1/2 min-h-0 shrink-0 flex-col gap-2 px-2 pb-2 pt-1">
      {said && <AnswerCard said={said} onClose={() => setSaid(null)} />}
      <form
        className="ui-field flex shrink-0 items-end gap-1 rounded-md py-1 pl-3 pr-1"
        onSubmit={(event) => {
          event.preventDefault();
          send();
        }}
      >
        <Sparkles
          size={15}
          strokeWidth={1.75}
          aria-hidden="true"
          className="mb-2 shrink-0 text-muted"
        />
        <textarea
          ref={field}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            const plain = !event.shiftKey && !event.metaKey && !event.ctrlKey;
            if (event.key === "Enter" && plain && !event.nativeEvent.isComposing) {
              event.preventDefault();
              send();
            } else if (event.key === "Escape") {
              // The field lets go first; the list's own Esc comes after.
              event.stopPropagation();
              if (said && !thinking) setSaid(null);
              else field.current?.blur();
            }
          }}
          rows={1}
          aria-label="Ask Postrider"
          placeholder="Ask Postrider, or tell it what to do"
          className="field-sizing-content max-h-40 min-w-0 flex-1 resize-none bg-transparent py-1.5 outline-none placeholder:text-faint"
        />
        <button
          type="submit"
          disabled={!text.trim() || thinking}
          aria-label="Send"
          title="Send (↩)"
          className="ui-button ui-icon-button shrink-0"
        >
          <ArrowUp size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </form>
    </div>
  );
}

/** The question, and Postrider's answer to it: what it did, with Undo. */
function AnswerCard({ said, onClose }: { said: Said; onClose: () => void }) {
  const { answer, problem } = said;
  return (
    <section
      aria-label="Postrider's answer"
      aria-live="polite"
      className="flex min-h-0 flex-col gap-2.5 overflow-y-auto rounded-lg bg-surface px-4 py-3"
    >
      <div className="flex items-start gap-2">
        <p className="min-w-0 flex-1 text-xs text-faint">{said.question}</p>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close the answer"
          title="Close (esc)"
          className="ui-button ui-icon-button -my-1 -mr-2 shrink-0"
        >
          <X size={13} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </div>
      {!answer && !problem && <p className="text-faint">Postrider is thinking…</p>}
      <ErrorMessage>{problem}</ErrorMessage>
      {answer && <Answered answer={answer} />}
    </section>
  );
}

function Answered({ answer }: { answer: Answer }) {
  const { workspace, place } = useWorkspace();
  const { folders, moved } = useFolders();
  const navigate = useNavigate();
  const shown = (useParams({ strict: false }) as { folder?: string }).folder;
  const [undo, setUndo] = useState<"undoing" | "undone">();
  const [problem, setProblem] = useState("");
  const undone = undo === "undone";

  // Undo takes away a folder the answer made, once it's empty again: shown,
  // the Inbox shows in its place. Only the folders read right after Undo
  // tell; one of that name made again later stays open.
  const made = answer.folder?.key;
  const latest = useRef(folders);
  latest.current = folders;
  const before = useRef<typeof folders | null>(null);
  useEffect(() => {
    if (!before.current || before.current === folders) return;
    before.current = null;
    if (made !== undefined && shown === made && !folders.some((f) => f.key === made)) {
      void navigate({ to: "/$workspace", params: { workspace: workspace.url_key } });
    }
  }, [folders, shown, made, navigate, workspace.url_key]);

  const putBack = (id: string) => {
    setUndo("undoing");
    setProblem("");
    api("POST", `/api/w/${workspace.id}/answers/${id}/undo`, undefined, place).then(
      () => {
        before.current = latest.current;
        setUndo("undone");
        moved();
      },
      (error) => {
        setProblem(messageOf(error));
        setUndo(undefined);
      },
    );
  };

  return (
    <>
      <div className="flex items-start gap-2.5">
        {answer.folder ? (
          <Folder
            size={15}
            strokeWidth={1.75}
            aria-hidden="true"
            className="mt-0.5 shrink-0 text-muted"
          />
        ) : (
          <Sparkles
            size={15}
            strokeWidth={1.75}
            aria-hidden="true"
            className="mt-0.5 shrink-0 text-muted"
          />
        )}
        <div className="flex min-w-0 flex-col gap-2">
          {answer.text.split(/\n{2,}/).map((paragraph, index) => (
            // oxlint-disable-next-line react/no-array-index-key -- an answer's words don't move
            <p key={index} className="whitespace-pre-wrap">
              {bold(paragraph)}
            </p>
          ))}
        </div>
      </div>
      {answer.chips.length > 0 && !undone && (
        <ul aria-label="What the rule looks for" className="flex flex-wrap gap-1.5 pl-6">
          {answer.chips.map((chip) => (
            <li key={chip} className="ui-chip flex h-6 items-center px-2.5">
              {chip}
            </li>
          ))}
        </ul>
      )}
      {answer.id &&
        (undone ? (
          <p className="pl-6 text-xs text-muted">Undone.</p>
        ) : (
          <div className="flex items-center gap-3 pl-6">
            <Button
              variant="secondary"
              disabled={undo === "undoing"}
              onClick={() => answer.id && putBack(answer.id)}
            >
              {undo === "undoing" ? "Undoing…" : "Undo"}
            </Button>
            {answer.folder && (
              <Link
                to="/$workspace/folders/$folder"
                params={{ workspace: workspace.url_key, folder: answer.folder.key }}
                className="ui-link"
              >
                Open {answer.folder.name}
              </Link>
            )}
          </div>
        ))}
      <ErrorMessage>{problem}</ErrorMessage>
    </>
  );
}

/** A line as Postrider wrote it, with the names it put in **bold**. */
function bold(text: string): ReactNode[] {
  // The bold names sit at the odd places the split leaves them.
  return text.split(/\*\*([^*]+)\*\*/).map((part, index) =>
    index % 2 === 0 ? (
      part
    ) : (
      // oxlint-disable-next-line react/no-array-index-key -- an answer's words don't move
      <b key={index}>{part}</b>
    ),
  );
}
