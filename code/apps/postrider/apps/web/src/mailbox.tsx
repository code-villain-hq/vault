import { useKeptWidth, WidthEdge, type Widths } from "@code-villain/web";
import { Outlet, useLocation, useParams, useSearch } from "@tanstack/react-router";
import { type CSSProperties, useMemo } from "react";
import { FolderList, SearchList } from "./folders";
import { type From, InboxList, mailKey, NoAccounts } from "./inbox";
import { PageBody } from "./layout";
import { useAccounts } from "./mail";
import { Empty } from "./ui";
import { useSyncStatus, useWorkspace } from "./workspace";

/** How wide the list starts beside the mail, and how narrow and wide its edge sets it. */
const LIST_WIDTH: Widths = { start: 440, min: 320, max: 720 };

/**
 * The Inbox, a folder or a search: its list in the middle, and the mail
 * opened from it on the right (the child route). The list stays while
 * mails open beside it, where it was scrolled to. A narrow window shows one
 * of them at a time.
 */
export function MailboxLayout() {
  const { ready } = useSyncStatus();
  const accounts = useAccounts();
  const { from, open } = useShownList();
  const width = useKeptWidth("list", LIST_WIDTH);
  if (!ready) return null;
  if (!accounts.length) return <NoAccounts />;
  return (
    <div
      className="mailbox flex min-h-0 flex-1 gap-1"
      data-reading={open === undefined ? undefined : ""}
      style={{ "--list-width": `${width.width}px` } as CSSProperties}
    >
      <section aria-label="Mail" className="mail-list flex min-h-0 min-w-0 shrink-0 flex-col">
        <ListColumn from={from} open={open} />
      </section>
      <WidthEdge
        width={width.width}
        onChange={width.setWidth}
        widths={LIST_WIDTH}
        label="List width"
      />
      <section aria-label="Open mail" className="mail-reader flex min-h-0 min-w-0 flex-1 flex-col">
        <Outlet />
      </section>
    </div>
  );
}

function ListColumn({ from, open }: { from: From; open: string | undefined }) {
  if (from.q !== undefined) return <SearchList q={from.q} open={open} />;
  if (from.folder !== undefined) return <FolderList folder={from.folder} open={open} />;
  return <InboxList open={open} />;
}

/**
 * The list the page shows, by its route, and the mail open from it, by
 * its key in that list: the Inbox, a folder, a search, or a mail's `From`.
 */
function useShownList(): { from: From; open?: string } {
  const { workspace } = useWorkspace();
  const { pathname } = useLocation();
  const { folder, account, uid } = useParams({ strict: false }) as {
    folder?: string;
    account?: string;
    uid?: string;
  };
  const search = useSearch({ strict: false }) as From;
  const searching = pathname === `/${workspace.url_key}/search`;
  return useMemo(() => {
    if (account !== undefined && uid !== undefined) {
      return {
        from: { folder: search.folder, q: search.q },
        open: mailKey({ folder: search.folder, account_id: account, uid: Number(uid) }),
      };
    }
    if (folder !== undefined) return { from: { folder } };
    if (searching) return { from: { q: search.q ?? "" } };
    return { from: {} };
  }, [account, uid, folder, searching, search.folder, search.q]);
}

/** The right-hand column with no mail open in it. */
export function NoMailOpen() {
  return (
    <>
      <div className="app-topbar min-h-12 shrink-0" data-tauri-drag-region="deep" />
      <PageBody>
        <Empty title="No mail open.">
          <p>Pick one from the list, or move to it with J and K and open it with Enter.</p>
        </Empty>
      </PageBody>
    </>
  );
}
