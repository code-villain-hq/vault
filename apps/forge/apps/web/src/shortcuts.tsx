// "G" then a letter goes to a page, as in Linear; "?" lists every shortcut.
import { Dialog } from "@base-ui/react/dialog";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { isTyping } from "./new-issue";
import { useWorkspace } from "./workspace";

const GO_TO: Record<string, { to: string; name: string }> = {
  i: { to: "/$workspace/inbox", name: "Inbox" },
  m: { to: "/$workspace/my-issues", name: "My issues" },
  t: { to: "/$workspace/triage", name: "Triage" },
  p: { to: "/$workspace/projects", name: "Projects" },
  n: { to: "/$workspace/initiatives", name: "Initiatives" },
  s: { to: "/$workspace/services", name: "Services" },
  e: { to: "/$workspace/domains", name: "Domains" },
  d: { to: "/$workspace/decisions", name: "Decisions" },
};

const SECTIONS: { title: string; keys: [string, string][] }[] = [
  {
    title: "Anywhere",
    keys: [
      ["⌘K", "Command menu and search"],
      ["C", "New issue"],
      ["?", "These shortcuts"],
      ...Object.entries(GO_TO).map(([key, { name }]): [string, string] => [
        `G then ${key.toUpperCase()}`,
        `Go to ${name}`,
      ]),
    ],
  },
  {
    title: "Issue lists and boards",
    keys: [
      ["J K or ↓ ↑", "Next and previous issue"],
      ["H L or ← →", "Previous and next column (board)"],
      ["⇧ ← →", "Move the issue to the previous or next status (board)"],
      ["→ ←", "Show or hide the issue's sub-issues (list)"],
      ["Enter", "Open the issue"],
      ["X", "Select the issue, to change several at once (list)"],
      ["Esc", "Clear the selection"],
      ["F", "Filter"],
    ],
  },
  {
    title: "An issue",
    keys: [
      ["Esc", "Back to where you came from"],
      ["1 2 3", "In triage: accept, mark as a duplicate, decline"],
      ["⌘ ⇧ .", "Copy the git branch name"],
      ["⌘ Enter", "Send a comment"],
    ],
  },
  {
    title: "Menus",
    keys: [
      ["Type", "Search the options"],
      ["Enter", "Choose the highlighted or first match"],
    ],
  },
];

/** Handles "G" sequences and "?"; shows the list of shortcuts. */
/** Opens the list of keyboard shortcuts, as "?" does. */
export function openShortcuts() {
  window.dispatchEvent(new Event("forge:shortcuts"));
}

export function Shortcuts() {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let pendingG = 0;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
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
      const target = GO_TO[key];
      if (target && event.timeStamp - pendingG < 1500) {
        event.preventDefault();
        pendingG = 0;
        void navigate({ to: target.to, params: { workspace: workspace.url_key } } as never);
      }
    };
    const onOpen = () => setOpen(true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("forge:shortcuts", onOpen);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("forge:shortcuts", onOpen);
    };
  }, [navigate, workspace.url_key]);

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-40 bg-black/40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup className="app-dialog fixed top-[10vh] left-1/2 z-40 max-h-[80vh] w-[min(34rem,calc(100vw-2rem))] -translate-x-1/2 overflow-y-auto rounded-lg bg-surface p-5 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
          <Dialog.Title className="mb-4 font-medium">Keyboard shortcuts</Dialog.Title>
          <div className="flex flex-col gap-5">
            {SECTIONS.map((section) => (
              <section key={section.title}>
                <h3 className="mb-2 text-xs font-medium text-muted">{section.title}</h3>
                <dl className="flex flex-col gap-1.5">
                  {section.keys.map(([keys, what]) => (
                    <div key={keys + what} className="flex items-center justify-between gap-4">
                      <dt className="text-muted">{what}</dt>
                      <dd className="flex shrink-0 items-center gap-1 text-xs text-faint">
                        {keys.split(/\s+/).map((key) =>
                          key === "then" || key === "or" ? (
                            <span key={key}>{key}</span>
                          ) : (
                            <kbd
                              key={key}
                              className="min-w-5 rounded bg-bg px-1 py-0.5 text-center font-sans text-fg"
                            >
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
