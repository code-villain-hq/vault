// Parcels: every tracking number the mail on this Mac gives, and every
// order a shop says is on its way before it gives one, the ones still
// coming first, each with where it is as its newest mail says and a link
// to follow it on the carrier's page or the shop's. Nothing asks a
// carrier: the state is the mail's, so it says when that mail came. The
// shop's name opens the mail.

import { messageOf } from "@code-villain/web";
import type { Parcel, ParcelState, Parcels } from "@postrider/types";
import { Link } from "@tanstack/react-router";
import { ExternalLink } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { initial, useNow } from "./inbox";
import { PageBody, PageHeader } from "./layout";
import { hue, mailTime, useAccounts } from "./mail";
import { isDesktop, openInBrowser } from "./platform";
import { Empty, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

const STATES: Record<ParcelState, string> = {
  shipped: "On its way",
  out_for_delivery: "Out for delivery",
  pick_up: "Ready to pick up",
  delivered: "Delivered",
};

/** What was read last, so the page shows it at once when it opens again. */
const lastRead = new Map<string, Parcels>();

export function ParcelsPage() {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const byId = useMemo(() => new Map(accounts.map((a) => [a.id, a])), [accounts]);
  const now = useNow();
  const [data, setData] = useState(() => lastRead.get(workspace.id));
  const [error, setError] = useState("");
  useEffect(() => {
    api<Parcels>("GET", `/api/w/${workspace.id}/parcels`, undefined, place).then(
      (read) => {
        lastRead.set(workspace.id, read);
        setData(read);
        setError("");
      },
      (problem) => setError(messageOf(problem)),
    );
  }, [workspace.id, place]);

  return (
    <>
      <PageHeader title="Parcels" />
      <PageBody>
        <div className="parcels">
          {!data && !error && <p className="px-3 text-muted">Reading your mail for parcels…</p>}
          <ErrorMessage>{error}</ErrorMessage>
          {data && data.parcels.length === 0 && (
            <Empty title="No parcels on their way">
              When a shop or a carrier mails you a tracking number, or a shop says your order is on
              its way, the parcel shows here, with where it is and a link to follow it.
            </Empty>
          )}
          {data && data.parcels.length > 0 && (
            <ol className="pc-table" aria-label="Parcels">
              {data.parcels.map((parcel) => (
                <ParcelRow
                  key={parcel.number ?? `${parcel.mail.from_address} ${parcel.order}`}
                  parcel={parcel}
                  accountHue={byId.get(parcel.mail.account_id)?.hue ?? 1}
                  workspace={workspace.url_key}
                  now={now}
                />
              ))}
            </ol>
          )}
        </div>
      </PageBody>
    </>
  );
}

function ParcelRow({
  parcel,
  accountHue,
  workspace,
  now,
}: {
  parcel: Parcel;
  accountHue: number;
  workspace: string;
  now: Date;
}) {
  const { mail } = parcel;
  const shop = mail.from_name || mail.from_address;
  const waiting = parcel.state === "out_for_delivery" || parcel.state === "pick_up";
  return (
    <li className={`pc ${parcel.state === "delivered" ? "done" : ""}`} style={hue(accountHue)}>
      <span className="av" aria-hidden="true">
        {initial(shop)}
      </span>
      <span className="who">
        <Link
          to="/$workspace/mail/$account/$uid"
          params={{ workspace, account: mail.account_id, uid: String(mail.uid) }}
          search={{ folder: mail.folder ?? undefined }}
          className="name"
          title={mail.subject}
        >
          {shop}
        </Link>
        <span className="num">
          {parcel.carrier ? `${parcel.carrier} · ` : ""}
          {parcel.number ?? `Order ${parcel.order}`}
        </span>
      </span>
      <span className={`st ${waiting ? "waiting" : ""}`}>{STATES[parcel.state]}</span>
      <span className="lw" title="When the mail that says so came">
        {mailTime(mail.date, now)}
      </span>
      {parcel.track_url && <TrackLink url={parcel.track_url} />}
    </li>
  );
}

function TrackLink({ url }: { url: string }) {
  return (
    <a
      className="ui-button track"
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(event) => {
        if (!isDesktop) return;
        event.preventDefault();
        void openInBrowser(url);
      }}
    >
      Track
      <ExternalLink size={13} strokeWidth={1.75} aria-hidden="true" />
    </a>
  );
}
