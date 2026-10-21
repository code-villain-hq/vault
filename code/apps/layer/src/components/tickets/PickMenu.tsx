import { useState } from "react";
import { ContextMenu } from "../ui";

/// A labelled button that opens a menu of choices, the current one ticked:
/// Sort and Group in the Tickets header.
export function PickMenu<T extends string>({
  label,
  options,
  names,
  value,
  onChange,
}: {
  label: string;
  options: readonly T[];
  names: Record<T, string>;
  value: T;
  onChange: (next: T) => void;
}) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <span className="sub sort-label">{label}</span>
      <button
        type="button"
        className="ui-button"
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setAt({ x: r.left, y: r.bottom + 4 });
        }}
      >
        {names[value]} ▾
      </button>
      {at && (
        <ContextMenu
          x={at.x}
          y={at.y}
          items={options.map((o) => ({
            label: `${o === value ? "✓ " : ""}${names[o]}`,
            onSelect: () => onChange(o),
          }))}
          onClose={() => setAt(null)}
        />
      )}
    </>
  );
}
