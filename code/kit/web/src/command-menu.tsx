import { Dialog } from "@base-ui/react/dialog";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useDialogFocus } from "./dialog-focus";

/** One line of the command menu. */
export interface Command {
  id: string;
  /** The heading it sits under: "Actions", "Go to", "Settings", "Workspaces"… */
  group: string;
  label: string;
  /** Its shortcut, shown at the right ("G I"), or an issue's key. */
  hint?: string;
  icon?: ReactNode;
  run: () => void;
}

const OPEN_EVENT = "code-villain:command-menu";

/** Opens the command menu, as ⌘K does. */
export function openCommandMenu() {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

/**
 * ⌘K: actions on what's in front of you, create, go to, settings,
 * workspaces, sign out, and search (behaviour.md). The app gives the lines:
 * `useCommands` is a hook, called while the menu is open, that returns what
 * matches `query` (everything when it's empty), in the order to show.
 */
export function CommandMenu({
  useCommands,
  placeholder,
}: {
  useCommands: (query: string) => Command[];
  placeholder: string;
}) {
  const [open, setOpen] = useState(false);
  const finalFocus = useDialogFocus(open);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setOpen((o) => !o);
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

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="ui-backdrop fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup
          finalFocus={finalFocus}
          className="ui-dialog fixed top-[14vh] left-1/2 z-40 w-[min(36rem,calc(100vw-2rem))] -translate-x-1/2 overflow-hidden outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"
        >
          <Dialog.Title className="sr-only">Command menu</Dialog.Title>
          {open && (
            <Commands
              useCommands={useCommands}
              placeholder={placeholder}
              onDone={() => setOpen(false)}
            />
          )}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Commands({
  useCommands,
  placeholder,
  onDone,
}: {
  useCommands: (query: string) => Command[];
  placeholder: string;
  onDone: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const results = useCommands(query.trim().toLowerCase());

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const run = (command: Command | undefined) => {
    if (!command) return;
    onDone();
    command.run();
  };

  let lastGroup = "";
  return (
    <div>
      <input
        // biome-ignore lint/a11y/noAutofocus: the menu opens to type into it
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((i) => Math.min(i + 1, results.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => Math.max(i - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            run(results[active]);
          }
        }}
        placeholder={placeholder}
        aria-label="Command"
        role="combobox"
        aria-expanded="true"
        aria-controls="command-list"
        aria-activedescendant={results[active] ? `command-${results[active].id}` : undefined}
        className="h-14 w-full bg-transparent px-4 text-query outline-none placeholder:text-faint"
      />
      <div
        ref={listRef}
        id="command-list"
        role="listbox"
        className="max-h-96 overflow-y-auto p-1.5"
      >
        {results.length === 0 && (
          <p className="px-3 py-6 text-center text-muted">Nothing matches that.</p>
        )}
        {results.map((command, index) => {
          const header = command.group !== lastGroup ? command.group : null;
          lastGroup = command.group;
          return (
            <div key={command.id}>
              {header && <div className="px-2.5 pt-2 pb-1 text-xs text-faint">{header}</div>}
              <div
                id={`command-${command.id}`}
                role="option"
                aria-selected={index === active}
                tabIndex={-1}
                data-index={index}
                onMouseMove={() => setActive(index)}
                onClick={() => run(command)}
                onKeyDown={() => {}}
                className="ui-option flex h-9 cursor-default items-center gap-2.5 px-2.5"
              >
                {command.icon}
                <span className="truncate">{command.label}</span>
                {command.hint && (
                  <span className="ml-auto font-mono text-xs text-faint">{command.hint}</span>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The commands whose label holds `query`, all of them when it's empty. */
export function matching(commands: Command[], query: string): Command[] {
  return query ? commands.filter((c) => c.label.toLowerCase().includes(query)) : commands;
}
