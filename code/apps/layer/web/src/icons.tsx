import type { LucideIcon } from "lucide-react";

/** A sidebar icon, in Lucide's size and stroke. */
export function NavIcon({ icon: Icon }: { icon: LucideIcon }) {
  return <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />;
}

/** A name's first letter in a small badge: round for people, square for workspaces. */
export function Initial({
  name,
  square = false,
  size = "size-5",
  title,
}: {
  name: string;
  square?: boolean;
  size?: string;
  title?: string;
}) {
  return (
    <span
      className={`${size} ui-control flex shrink-0 items-center justify-center bg-chip text-muted ${square ? "rounded-sm" : "rounded-full"}`}
      aria-hidden="true"
      title={title}
    >
      {name.trim().charAt(0).toUpperCase()}
    </span>
  );
}
