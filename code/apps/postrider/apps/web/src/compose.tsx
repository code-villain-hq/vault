import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog } from "@code-villain/web";
import type { MailSummary, Message, Reply } from "@postrider/types";
import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { api, messageOf } from "./api";
import { initial, mailKey } from "./inbox";
import { hue, useAccounts } from "./mail";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** What was written to each mail and not sent: Esc keeps it. */
const drafts = new Map<string, { text: string; from?: string }>();

/**
 * R: a reply to a mail, written in a dialog over it. It goes to whom the
 * mail asks replies to go, from the account it came to unless you pick
 * another; ⌘Enter sends it, and Esc closes it with what you wrote kept.
 */
export function ReplyDialog({
  open,
  onClose,
  summary,
  message,
}: {
  open: boolean;
  onClose: () => void;
  summary: MailSummary;
  message: Message | undefined;
}) {
  return (
    <CreateDialog open={open} onClose={onClose} width="40rem">
      <ReplyForm summary={summary} message={message} onSent={onClose} />
    </CreateDialog>
  );
}

function ReplyForm({
  summary,
  message,
  onSent,
}: {
  summary: MailSummary;
  message: Message | undefined;
  onSent: () => void;
}) {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const key = mailKey(summary);
  const kept = drafts.get(key);
  const [text, setText] = useState(kept?.text ?? "");
  const [from, setFrom] = useState(kept?.from ?? summary.account_id);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string>();
  const sender = accounts.find((a) => a.id === from);
  const mine = accounts.find((a) => a.id === summary.account_id);
  const to = message?.reply_to ?? summary.from_name;

  const keep = (next: { text?: string; from?: string }) => {
    const draft = { text, from, ...next };
    if (draft.text.trim()) drafts.set(key, draft);
    else drafts.delete(key);
  };

  const send = () => {
    if (sending || !text.trim()) return;
    setSending(true);
    setError(undefined);
    const reply: Reply = {
      text,
      from_account: from === summary.account_id ? undefined : from,
      folder: summary.folder ?? undefined,
    };
    api(
      "POST",
      `/api/w/${workspace.id}/mails/${summary.account_id}/${summary.uid}/reply`,
      reply,
      place,
    ).then(
      () => {
        drafts.delete(key);
        onSent();
      },
      (problem: unknown) => {
        setSending(false);
        setError(messageOf(problem));
      },
    );
  };

  return (
    <form
      className="compose"
      onSubmit={(event) => {
        event.preventDefault();
        send();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          send();
        }
      }}
    >
      <Dialog.Title className="sr-only">Reply to {to}</Dialog.Title>
      <div className="compose-line">
        <span className="compose-label">To</span>
        <span className="ui-chip compose-to" style={hue(mine?.hue ?? 1)}>
          <span className="av sm" aria-hidden="true">
            {initial(to)}
          </span>
          {to}
        </span>
      </div>
      <div className="compose-line">
        <span className="compose-label">Subject</span>
        <span className="truncate">{replySubject(summary.subject)}</span>
      </div>
      <textarea
        autoFocus
        aria-label="Your reply"
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          keep({ text: event.target.value });
        }}
        className="compose-text"
      />
      {error && <ErrorMessage>{error}</ErrorMessage>}
      <div className="compose-foot">
        {accounts.length > 1 ? (
          <label className="compose-from" style={hue(sender?.hue ?? 1)}>
            <span className="av sm" aria-hidden="true">
              {initial(sender?.name ?? "")}
            </span>
            <select
              aria-label="From"
              value={from}
              onChange={(event) => {
                setFrom(event.target.value);
                keep({ from: event.target.value });
              }}
            >
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.address}
                </option>
              ))}
            </select>
            <ChevronDown size={14} strokeWidth={1.75} aria-hidden="true" />
          </label>
        ) : (
          <span className="compose-from" style={hue(sender?.hue ?? 1)}>
            <span className="av sm" aria-hidden="true">
              {initial(sender?.name ?? "")}
            </span>
            {sender?.address}
          </span>
        )}
        <Button
          type="submit"
          className="ml-auto"
          disabled={sending || !text.trim()}
          title="Send (⌘Enter)"
        >
          {sending ? "Sending…" : "Send"}
        </Button>
      </div>
    </form>
  );
}

/** "Re: " the subject, unless it's a reply already. */
function replySubject(subject: string): string {
  return /^(re|aw):/i.test(subject.trim()) ? subject.trim() : `Re: ${subject.trim()}`;
}
