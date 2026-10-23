import type { TabInfo } from "../lib/types";
import { CloseIcon, PlusIcon } from "./icons";
import { Spinner } from "./ui";

/**
 * Small details, everywhere: shared tab styling, keyboard selection and names.
 * A task's browser tabs (BRW-16): one active, shared with its agents, named
 * by their page's title (BRW-17). Does nothing while an agent is using the
 * browser (BRW-15): the user takes it over first.
 */
export function BrowserTabs({
  tabs,
  locked,
  onSwitch,
  onClose,
  onNew,
}: {
  tabs: TabInfo[];
  locked: boolean;
  onSwitch: (id: string) => void;
  onClose: (id: string) => void;
  onNew: () => void;
}) {
  return (
    <div className={`browser-tabs ui-tabs${locked ? " locked" : ""}`} role="tablist" aria-label="Browser tabs">
      {tabs.map((t) => (
        <div
          key={t.id}
          className={`browser-tab${t.active ? " active" : ""}`}
          title={`${t.name}\n${t.url}`}
          onAuxClick={(e) => { if (!locked && e.button === 1) onClose(t.id); }}
        >
          <button
            type="button"
            role="tab"
            className="ui-tab browser-tab-name"
            aria-selected={t.active}
            tabIndex={t.active ? 0 : -1}
            disabled={locked}
            onClick={() => { if (!t.active) onSwitch(t.id); }}
            onKeyDown={(event) => {
              const at = tabs.findIndex((tab) => tab.id === t.id);
              const next = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1
                : event.key === "ArrowRight" ? (at + 1) % tabs.length
                : event.key === "ArrowLeft" ? (at - 1 + tabs.length) % tabs.length : -1;
              if (next < 0) return;
              event.preventDefault();
              const tab = tabs[next];
              if (tab) onSwitch(tab.id);
              event.currentTarget.closest('[role="tablist"]')?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
            }}
          >
            {t.loading && <Spinner />}
            <span className="name">{t.name}</span>
          </button>
          <button
            className="x"
            title="Close this tab"
            disabled={locked}
            onClick={(e) => { e.stopPropagation(); onClose(t.id); }}
          >
            <CloseIcon />
          </button>
        </div>
      ))}
      <button className="ui-button ui-icon-button browser-tab-new" title="New tab" disabled={locked} onClick={onNew}>
        <PlusIcon />
      </button>
    </div>
  );
}
