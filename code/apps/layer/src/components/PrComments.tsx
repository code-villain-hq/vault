import type { ReactNode } from "react";
import { externalHttpUrl } from "../lib/external-url";
import type { PrDetailRead } from "../lib/pr-detail-read";
import type { FeedbackNote, PrComments as Snapshot, ReadState, ReviewThread } from "../lib/types";
import { Markdown } from "./Markdown";
import { PrCodeExcerpt } from "./PrCodeExcerpt";
import { ErrorMessage } from "./ui";

type OpenLink = (url: string) => void;

const reviewVerbs = new Map([
  ["APPROVED", "approved"],
  ["CHANGES_REQUESTED", "requested changes"],
  ["COMMENTED", "commented"],
  ["DISMISSED", "dismissed"],
]);

function Note({ note, onOpen }: { note: FeedbackNote; onOpen: OpenLink }) {
  const url = externalHttpUrl(note.url);
  return (
    <div className="fb-comment">
      <div className="row pr-note-heading">
        <span className="fb-author">
          {note.author}
          {note.state && ` ${reviewVerbs.get(note.state) ?? "reviewed"}`}
        </span>
        {url && (
          <a
            className="ui-link"
            href={url}
            aria-label={`Open ${note.author}'s note on GitHub`}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onOpen(url);
            }}
          >
            GitHub ↗
          </a>
        )}
      </div>
      <Markdown text={note.body} />
    </div>
  );
}

function Notes({ notes, onOpen }: { notes: FeedbackNote[]; onOpen: OpenLink }) {
  return notes.map((note, index) => <Note key={note.url || index} note={note} onOpen={onOpen} />);
}

function Thread({ thread, onOpen }: { thread: ReviewThread; onOpen: OpenLink }) {
  const { path, line, start_line: start } = thread;
  const range =
    start !== null && line !== null && start < line ? `${start}-${line}` : (line ?? start);
  const heading = (
    <>
      <code>{range === null ? path : `${path}:${range}`}</code>
      {thread.resolved && <span className="ui-chip">Resolved</span>}
      {thread.outdated && <span className="ui-chip">Outdated</span>}
    </>
  );
  const body = (
    <div className="pr-thread-body">
      {thread.code.length > 0 && <PrCodeExcerpt lines={thread.code} />}
      <Notes notes={thread.comments} onOpen={onOpen} />
    </div>
  );
  return thread.resolved ? (
    <details className="pr-thread">
      <summary className="pr-thread-heading">{heading}</summary>
      {body}
    </details>
  ) : (
    <article className="pr-thread">
      <div className="pr-thread-heading">{heading}</div>
      {body}
    </article>
  );
}

function CommentSection({
  heading,
  noun,
  state,
  count,
  children,
}: {
  heading: string;
  noun: string;
  state: ReadState;
  count: number;
  children: ReactNode;
}) {
  return (
    <section>
      <h4>{heading}</h4>
      {state === "incomplete" && (
        <p className="ui-tint-warn error-message" role="status">
          The {noun} list is incomplete. Try again.
        </p>
      )}
      {state === "unavailable" && (
        <ErrorMessage>Couldn’t read {heading.toLowerCase()}. Try again.</ErrorMessage>
      )}
      {state === "complete" && count === 0 && <p className="muted">No {heading.toLowerCase()}.</p>}
      {count > 0 && (
        // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- keyboard users need to be able to scroll the whole section
        <div className="pr-comment-items" role="region" aria-label={heading} tabIndex={0}>
          {children}
        </div>
      )}
    </section>
  );
}

/** The read-only comments surface, shared by task and Reviews PR cards. */
export function PrComments({ read, onOpen }: { read: PrDetailRead<Snapshot>; onOpen: OpenLink }) {
  if (read.status === "idle") return null;
  return (
    <div className="pr-comments">
      {read.status === "loading" && (
        <p className="muted" role="status">
          Reading comments…
        </p>
      )}
      {read.status === "failed" && <ErrorMessage>{read.message}</ErrorMessage>}
      {read.status === "ready" && (
        <>
          <CommentSection
            heading="Conversation comments"
            noun="conversation comment"
            state={read.data.comments_state}
            count={read.data.comments.length}
          >
            <Notes notes={read.data.comments} onOpen={onOpen} />
          </CommentSection>
          <CommentSection
            heading="Submitted review text"
            noun="submitted review"
            state={read.data.reviews_state}
            count={read.data.reviews.length}
          >
            <Notes notes={read.data.reviews} onOpen={onOpen} />
          </CommentSection>
          <CommentSection
            heading="Inline review threads"
            noun="inline review thread"
            state={read.data.threads_state}
            count={read.data.threads.length}
          >
            {read.data.threads.map((thread, index) => (
              <Thread key={thread.url || index} thread={thread} onOpen={onOpen} />
            ))}
          </CommentSection>
        </>
      )}
    </div>
  );
}
