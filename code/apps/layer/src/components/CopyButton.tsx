import { useCopied } from "../lib/clipboard";
import { CopyIcon } from "./icons";

/**
 * An icon button that copies `text` and reads "Copied" for a moment, in
 * place of a toast (NOTE-7). A press never reaches the card under it.
 */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const { copied, copy } = useCopied();
  return (
    <button
      className={`ui-button ui-icon-button${copied ? " copied" : ""}`}
      title={copied ? "Copied" : label}
      aria-label={copied ? "Copied" : label}
      onClick={(e) => { e.stopPropagation(); copy(text); }}
    >
      {copied ? "Copied" : <CopyIcon />}
    </button>
  );
}
