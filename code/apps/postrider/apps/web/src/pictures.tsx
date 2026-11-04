// Pictures from the web, held back: a mail's HTML that loads them as it
// opens tells its sender it was read, and when. So they wait for "Load
// pictures" on that mail, or "Always from" its sender, which the server
// keeps (`pictures.rs`) and Settings lists. Pictures sent in the mail
// itself show anyway, and so do those in your own mail.

import { messageOf } from "@code-villain/web";
import type { PictureSender, PictureSenders } from "@postrider/types";
import { ImageOff } from "lucide-react";
import { useEffect, useState, useSyncExternalStore } from "react";
import { api } from "./api";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** Mails whose pictures you loaded, by key, while Postrider runs. */
const loadedOnce = new Set<string>();
/** Senders said yes or no to since their mail was read, by address. */
const said = new Map<string, boolean>();
const listeners = new Set<() => void>();
let version = 0;
const changed = () => {
  version += 1;
  for (const listen of listeners) listen();
};
const subscribe = (listen: () => void) => {
  listeners.add(listen);
  return () => listeners.delete(listen);
};

/**
 * Whether the mail `key` from `address` loads its pictures: you loaded
 * them, or said its sender's always do since, or the server said so as it
 * opened (`fromServer`).
 */
export function useLoadsPictures(key: string, address: string, fromServer: boolean): boolean {
  useSyncExternalStore(subscribe, () => version);
  return loadedOnce.has(key) || (said.get(address.toLowerCase()) ?? fromServer);
}

/** Says whether `address`'s mail loads its pictures from now on. */
function always(workspace: string, address: string, load: boolean) {
  const sender: PictureSender = { address, load };
  return api("POST", `/api/w/${workspace}/pictures`, sender).then(() => {
    said.set(address.toLowerCase(), load);
    changed();
  });
}

const WEB = /^\s*(?:https?:)?\/\//i;
const WEB_IN_CSS = /url\(\s*['"]?\s*(?:https?:)?\/\/|@import/i;

/**
 * Whether `html` would load something from the web: a picture, a
 * background, a stylesheet or a font. Without any, nothing is held back,
 * and nothing needs saying.
 */
export function loadsFromWeb(html: string): boolean {
  const mail = new DOMParser().parseFromString(html, "text/html");
  for (const element of mail.querySelectorAll(
    "[src], [srcset], [background], [poster], link[href], image",
  )) {
    for (const name of ["src", "background", "poster", "href", "xlink:href"]) {
      const value = element.getAttribute(name);
      if (value && WEB.test(value)) return true;
    }
    const set = element.getAttribute("srcset");
    if (set?.split(",").some((each) => WEB.test(each))) return true;
  }
  for (const styled of mail.querySelectorAll("[style], style")) {
    const css =
      styled instanceof HTMLStyleElement ? styled.textContent : styled.getAttribute("style");
    if (css && WEB_IN_CSS.test(css)) return true;
  }
  return false;
}

/** Above a mail whose pictures wait: load them now, or always from its sender. */
export function PicturesHeld({
  mailKey,
  name,
  address,
}: {
  mailKey: string;
  name: string;
  address: string;
}) {
  const { workspace } = useWorkspace();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const load = () => {
    loadedOnce.add(mailKey);
    changed();
  };
  const loadAlways = () => {
    setBusy(true);
    setError(undefined);
    always(workspace.id, address, true).then(
      () => setBusy(false),
      (problem: unknown) => {
        setBusy(false);
        setError(messageOf(problem));
      },
    );
  };
  return (
    <div className="pictures-held">
      <ImageOff size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
      <p className="min-w-0 flex-1">
        Pictures are held back, so {name || address} can't see you opened this.
      </p>
      <div className="flex gap-2">
        <Button variant="secondary" onClick={load}>
          Load pictures
        </Button>
        <Button variant="secondary" disabled={busy} onClick={loadAlways} title={address}>
          Always from this sender
        </Button>
      </div>
      <ErrorMessage>{error}</ErrorMessage>
    </div>
  );
}

/** Settings: the senders whose pictures always load, each to take away. */
export function PictureSendersList() {
  const { workspace } = useWorkspace();
  const [senders, setSenders] = useState<string[]>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    api<PictureSenders>("GET", `/api/w/${workspace.id}/pictures`).then(
      (read) => setSenders(read.senders),
      (problem: unknown) => setError(messageOf(problem)),
    );
  }, [workspace.id]);

  const remove = (address: string) => {
    setError(undefined);
    always(workspace.id, address, false).then(
      () => setSenders((now) => now?.filter((each) => each !== address)),
      (problem: unknown) => setError(messageOf(problem)),
    );
  };

  return (
    <>
      {senders && senders.length === 0 && (
        <p className="text-muted">
          Every sender's pictures wait until you load them. On a mail, Always from this sender adds
          it here.
        </p>
      )}
      {senders && senders.length > 0 && (
        <ul className="flex flex-col gap-1">
          {senders.map((address) => (
            <li
              key={address}
              className="flex items-center gap-3 rounded-md bg-surface py-1.5 pr-1.5 pl-4"
            >
              <span className="min-w-0 flex-1 truncate">{address}</span>
              <Button variant="secondary" onClick={() => remove(address)}>
                Remove
              </Button>
            </li>
          ))}
        </ul>
      )}
      <ErrorMessage>{error}</ErrorMessage>
    </>
  );
}
