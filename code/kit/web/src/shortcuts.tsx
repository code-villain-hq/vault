// "G" then a letter goes to a page, as in Linear; "?" lists every shortcut;
// in the desktop app ⌘, opens settings.
import { Dialog } from "@base-ui/react/dialog";
import { useEffect, useRef, useState } from "react";
import { shortcutFree } from "./keys";

/** "G" then `key` goes to the page called `name`. */
export interface GoTo {
  key: string;
  name: string;
  go: () => void;
}

/** A part of the list: keys as written ("J K or ↓ ↑"), and what they do. */
export interface ShortcutSection {
  title: string;
  keys: [string, string][];
}

const OPEN_EVENT = "code-villain:shortcuts";

/** Opens the list of keyboard shortcuts, as "?" does. */
export function openShortcuts() {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

/**
 * Handles "?", "G" sequences and ⌘, and shows the list of shortcuts.
 * `anywhere` adds the app's own keys that work on every page (Forge's "C").
 */
export function Shortcuts({
  goTo,
  anywhere = [],
  sections = [],
  onSettings,
}: {
  goTo: GoTo[];
  anywhere?: [string, string][];
  sections?: ShortcutSection[];
  /** Given in the desktop app, where ⌘, opens settings. */
  onSettings?: () => void;
}) {
  const [open, setOpen] = useState(false);
  // The handler reads the latest pages without listening again on each render.
  const latest = useRef({ goTo, onSettings });
  latest.current = { goTo, onSettings };

  useEffect(() => {
    let pendingG = 0;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "," && event.metaKey && latest.current.onSettings) {
        event.preventDefault();
        latest.current.onSettings();
        return;
      }
      if (!shortcutFree(event)) return;
      if (event.key === "?") {
        event.preventDefault();
        setOpen(true);
        return;
      }
      const key = event.key.toLowerCase();
      if (key === "g") {
        pendingG = event.timeStamp;
        return;
      }
      const target = latest.current.goTo.find((page) => page.key === key);
      if (target && event.timeStamp - pendingG < 1500) {
        event.preventDefault();
        pendingG = 0;
        target.go();
      }
    };
    const onOpen = () => setOpen(true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener(OPEN_EVENT, onOpen);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener(OPEN_EVENT, onOpen);
    };
  }, []);

  const all: ShortcutSection[] = [
    {
      title: "Anywhere",
      keys: [
        ["⌘K", "Command menu and search"],
        ...anywhere,
        ["?", "These shortcuts"],
        ...(onSettings ? [["⌘,", "Settings"] as [string, string]] : []),
        ...goTo.map(({ key, name }): [string, string] => [
          `G then ${key.toUpperCase()}`,
          `Go to ${name}`,
        ]),
      ],
    },
    ...sections,
    {
      title: "Menus",
      keys: [
        ["Type", "Search the options"],
        ["Enter", "Choose the highlighted or first match"],
      ],
    },
  ];

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="ui-backdrop fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup className="ui-dialog fixed top-[10vh] left-1/2 z-40 max-h-[80vh] w-[min(34rem,calc(100vw-2rem))] -translate-x-1/2 overflow-y-auto p-5 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
          <Dialog.Title className="mb-4 font-medium">Keyboard shortcuts</Dialog.Title>
          <div className="flex flex-col gap-5">
            {all.map((section) => (
              <section key={section.title}>
                <h3 className="mb-2 text-xs font-medium text-muted">{section.title}</h3>
                <dl className="flex flex-col gap-1.5">
                  {section.keys.map(([keys, what]) => (
                    <div key={keys + what} className="flex items-center justify-between gap-4">
                      <dt className="text-muted">{what}</dt>
                      <dd className="flex shrink-0 items-center gap-1 text-xs text-faint">
                        {keys.split(/\s+/).map((key, index) =>
                          key === "then" || key === "or" ? (
                            // biome-ignore lint/suspicious/noArrayIndexKey: the words repeat
                            <span key={index}>{key}</span>
                          ) : (
                            // biome-ignore lint/suspicious/noArrayIndexKey: keys can repeat
                            <kbd key={index} className="ui-kbd">
                              {key}
                            </kbd>
                          ),
                        )}
                      </dd>
                    </div>
                  ))}
                </dl>
              </section>
            ))}
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
