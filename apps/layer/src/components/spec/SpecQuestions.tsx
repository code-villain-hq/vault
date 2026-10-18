import { useEffect, useState } from "react";
import { questions, sameAnswer, setAnswer, type Question } from "../../lib/questions";
import { editorText, useSpecs } from "../../lib/specs";
import { useStore } from "../../store";
import type { AppliedAnswers, TaskView } from "../../lib/types";
import { Spinner } from "../ui";

/**
 * Beside the requirements editor: its open questions, each to answer by
 * picking one of the answers drafted for it or writing one's own (SPEC-19),
 * and Apply answers, which builds them into the requirements (SPEC-20). An
 * answer lives in the editor's text, so this list is only a view of it.
 */
export function SpecQuestions({ task }: { task: TaskView }) {
  const state = useSpecs((s) => s.byTask[task.id]);
  const edit = useSpecs((s) => s.edit);
  const applyAnswers = useSpecs((s) => s.applyAnswers);
  const undoAnswers = useSpecs((s) => s.undoAnswers);
  const fail = useStore((s) => s.fail);

  const text = editorText(state, "requirements", null);
  const qs = questions(text);
  const applied = state?.applied ?? null;
  const drafting = state?.drafting ?? null;
  const applying = drafting?.part === "requirements";
  const anyAnswered = qs.some((q) => q.answer);

  if (qs.length === 0 && !applied) return null;
  const open = qs.filter((q) => !q.answer).length;

  return (
    <section className="spec-questions">
      <h3>
        Open questions
        {qs.length > 0 && <span className="muted"> · {open === 0 ? "all answered" : `${open} of ${qs.length} unanswered`}</span>}
      </h3>
      {applied && (
        <div className="spec-applied">
          <span>{appliedSummary(applied.results)}</span>
          <button className="ui-button" disabled={!!drafting} onClick={() => undoAnswers(task.id)}>Undo</button>
        </div>
      )}
      {qs.map((q) => (
        <QuestionCard
          key={q.id}
          question={q}
          disabled={!!drafting}
          onAnswer={(answer) => edit(task.id, { part: "requirements", checkoutId: null }, setAnswer(text, q.id, answer))}
        />
      ))}
      {qs.length > 0 && (
        <div className="row spec-apply">
          <button
            className="ui-button"
            disabled={!anyAnswered || !!drafting}
            title={anyAnswered
              ? "Rewrite the requirements the answers decide, and take the answered questions out"
              : "Answer a question first"}
            onClick={() => void applyAnswers(task.id).catch(fail)}
          >
            Apply answers
          </button>
          {applying && <Spinner />}
        </div>
      )}
    </section>
  );
}

function QuestionCard({ question, disabled, onAnswer }: {
  question: Question;
  disabled: boolean;
  onAnswer: (answer: string) => void;
}) {
  const picked = question.options.find((o) => sameAnswer(o, question.answer));
  const ownAnswer = picked ? "" : (question.answer ?? "");
  // Kept here while typing, and written into the editor when it is left:
  // written at every key, the trimmed text read back ate each space.
  const [own, setOwn] = useState(ownAnswer);
  useEffect(() => setOwn(ownAnswer), [ownAnswer]);
  const commit = () => {
    if (own.trim() !== ownAnswer.trim()) onAnswer(own);
  };

  return (
    <div className={`spec-question${question.answer ? " answered" : ""}`}>
      <div className="spec-question-text">
        <b>{question.id}</b>
        <span>{question.text}</span>
        {question.answer && <span className="spec-question-done" aria-label="Answered">✓</span>}
      </div>
      {question.options.length > 0 && (
        <div className="spec-options">
          {question.options.map((o) => {
            const on = o === picked;
            return (
              <button
                key={o}
                className={`spec-option${on ? " active" : ""}`}
                disabled={disabled}
                aria-pressed={on}
                onClick={() => onAnswer(on ? "" : o)}
              >
                {o}
              </button>
            );
          })}
        </div>
      )}
      <input className="ui-field"
        value={own}
        disabled={disabled}
        placeholder={question.options.length > 0 ? "Or your own answer" : "Your answer"}
        onChange={(e) => setOwn(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
      />
    </div>
  );
}

/** "Changed R-4, R-6; added R-8.", or that none changed. */
function appliedSummary(r: AppliedAnswers): string {
  const parts = [
    r.changed.length > 0 && `changed ${r.changed.join(", ")}`,
    r.added.length > 0 && `added ${r.added.join(", ")}`,
    r.removed.length > 0 && `removed ${r.removed.join(", ")}`,
  ].filter(Boolean) as string[];
  if (parts.length === 0) return "No requirement changed.";
  const text = parts.join("; ");
  return text.charAt(0).toUpperCase() + text.slice(1) + ". Read it through before approving.";
}
