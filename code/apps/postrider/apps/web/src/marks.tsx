// A mail marked read or unread (⇧U), flagged or not (S): from a list's
// focused row, or the open mail. It shows at once, in the Inbox and every
// list kept here, and goes to its server, or waits for it while it can't
// be reached (`queued.rs`); a server that refuses puts it back and says so.
import type { MailSummary, MarkMail } from "@postrider/types";
import { api, messageOf } from "./api";
import { markListed } from "./folders";
import { type Row, unreadRow } from "./inbox";
import { useFolders, useInbox } from "./mail";
import { useWorkspace } from "./workspace";

/** Mail to mark, each as it says. */
export type Marking = { mail: MailSummary; marks: MarkMail }[];

/** A row's mail that the marks are about: a conversation's in the Inbox, else its one. */
function marked(row: Row): MailSummary[] {
  return row.conversation ? row.conversation.mails.filter((m) => m.folder === null) : [row.mail];
}

/** Whether a row is flagged: a conversation is while any of its Inbox mail is. */
export function flaggedRow(row: Row): boolean {
  return marked(row).some((mail) => mail.flagged);
}

/** ⇧U: an unread row read, all of it; a read one unread, the mail it opens. */
export function readToggle(row: Row): Marking {
  if (unreadRow(row)) {
    return marked(row)
      .filter((mail) => !mail.seen)
      .map((mail) => ({ mail, marks: { seen: true } }));
  }
  return [{ mail: row.mail, marks: { seen: false } }];
}

/** S: a flagged row unflagged, all of it; another flagged, the mail it opens. */
export function flagToggle(row: Row): Marking {
  if (flaggedRow(row)) {
    return marked(row)
      .filter((mail) => mail.flagged)
      .map((mail) => ({ mail, marks: { flagged: false } }));
  }
  return [{ mail: row.mail, marks: { flagged: true } }];
}

/** Marks mail as `Marking` says: here at once, then on its server. */
export function useMarkMail() {
  const { workspace, place, refusals } = useWorkspace();
  const { mark } = useInbox();
  const { refresh } = useFolders();
  return (marking: Marking) => {
    void Promise.allSettled(
      marking.map(({ mail, marks }) => {
        const inInbox = (mail.folder ?? "inbox").toLowerCase() === "inbox";
        const took = inInbox ? mark(mail.account_id, mail.uid, marks) : undefined;
        const putBack = markListed(mail, marks);
        const query =
          mail.folder === null ? "" : `?${new URLSearchParams({ folder: mail.folder })}`;
        return api(
          "POST",
          `/api/w/${workspace.id}/mails/${mail.account_id}/${mail.uid}/mark${query}`,
          marks,
          place,
        ).then(
          () => took?.(true),
          (problem: unknown) => {
            took?.(false);
            putBack();
            throw problem;
          },
        );
      }),
    ).then((done) => {
      // What's unread in the sidebar.
      refresh();
      const refused = done.find((one) => one.status === "rejected");
      if (refused) {
        refusals.dispatchEvent(new CustomEvent("refused", { detail: messageOf(refused.reason) }));
      }
    });
  };
}
