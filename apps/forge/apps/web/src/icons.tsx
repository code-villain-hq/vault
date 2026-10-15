import type { StateType, WorkflowState } from "@forge/types";
import { IconServer } from "@tabler/icons-react";
import type { LucideIcon } from "lucide-react";
import type { SVGProps } from "react";
import { readable } from "./contrast";
import { personColor, useReviewStateIds } from "./data";
import { ICONS } from "./icon-set";

/** A decorative 14×14 icon; the control around it carries the label. */
function Svg(props: SVGProps<SVGSVGElement>) {
  return <svg viewBox="0 0 14 14" className="size-3.5 shrink-0" aria-hidden="true" {...props} />;
}

/** A workflow state's icon, telling its team's review status apart. */
export function StateIcon({ state }: { state: WorkflowState }) {
  const review = useReviewStateIds().has(state.id);
  return <StatusIcon type={state.type} color={state.color} review={review} />;
}

/**
 * A workflow state's icon: its type decides the shape, its colour the
 * colour. The review status (`review`, see `reviewStateOf`) has a shape of
 * its own, a ring around a dot: done by whoever worked on it, now waiting
 * for someone to look. Not half full like work in progress, nor a check
 * like done.
 */
export function StatusIcon({
  type,
  color: chosen,
  review = false,
}: {
  type: StateType;
  color: string;
  review?: boolean;
}) {
  // The team's colour, adjusted where needed to stand out from the background.
  const color = readable(chosen);
  if (review) {
    return (
      <Svg fill="none" stroke={color} strokeWidth="1.5">
        <circle cx="7" cy="7" r="5.5" />
        <circle cx="7" cy="7" r="2.5" fill={color} stroke="none" />
      </Svg>
    );
  }
  switch (type) {
    case "triage":
      return (
        <Svg>
          <circle cx="7" cy="7" r="6.25" fill={color} />
          <path d="M4 7h6M8 5l2 2-2 2" fill="none" stroke="var(--surface)" strokeWidth="1.4" />
        </Svg>
      );
    case "backlog":
      return (
        <Svg fill="none" stroke={color} strokeWidth="1.5" strokeDasharray="2 2">
          <circle cx="7" cy="7" r="5.5" />
        </Svg>
      );
    case "unstarted":
      return (
        <Svg fill="none" stroke={color} strokeWidth="1.5">
          <circle cx="7" cy="7" r="5.5" />
        </Svg>
      );
    case "started":
      return (
        <Svg fill="none" stroke={color} strokeWidth="1.5">
          <circle cx="7" cy="7" r="5.5" />
          <path d="M7 3.5a3.5 3.5 0 0 1 0 7z" fill={color} stroke="none" />
        </Svg>
      );
    case "completed":
      return (
        <Svg>
          <circle cx="7" cy="7" r="6.25" fill={color} />
          <path
            d="m4.5 7.2 1.7 1.7 3.3-3.6"
            fill="none"
            stroke="var(--surface)"
            strokeWidth="1.5"
          />
        </Svg>
      );
    case "canceled":
      return (
        <Svg>
          <circle cx="7" cy="7" r="6.25" fill={color} />
          <path d="m5 5 4 4M9 5l-4 4" stroke="var(--surface)" strokeWidth="1.5" />
        </Svg>
      );
  }
}

/** 0 = none, 1 = urgent, 2 = high, 3 = medium, 4 = low. */
export function PriorityIcon({ priority }: { priority: number }) {
  if (priority === 0) {
    return (
      <Svg fill="var(--faint)">
        <rect x="2" y="6.25" width="2.5" height="1.5" rx=".5" />
        <rect x="5.75" y="6.25" width="2.5" height="1.5" rx=".5" />
        <rect x="9.5" y="6.25" width="2.5" height="1.5" rx=".5" />
      </Svg>
    );
  }
  if (priority === 1) {
    return (
      <Svg>
        <rect x="1" y="1" width="12" height="12" rx="3" fill="var(--danger)" />
        <path
          d="M7 3.75v4M7 9.75v.5"
          stroke="var(--surface)"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </Svg>
    );
  }
  const filled = 5 - priority; // high = 3 bars, medium = 2, low = 1
  return (
    <Svg>
      {[0, 1, 2].map((i) => (
        <rect
          key={i}
          x={2 + i * 3.75}
          y={9 - i * 3}
          width="2.5"
          height={3 + i * 3}
          rx=".5"
          fill={i < filled ? "var(--muted)" : "var(--line)"}
        />
      ))}
    </Svg>
  );
}

/** A person's initial in a coloured circle (a square for agents); dashed for nobody. */
export function Avatar({
  user,
  size = "size-5",
}: {
  user: { id: string; name: string; kind?: "person" | "agent" } | undefined;
  size?: string;
}) {
  if (!user) {
    return (
      <span
        className={`${size} shrink-0 rounded-full border border-dashed border-faint`}
        aria-hidden="true"
      />
    );
  }
  return (
    <span
      className={`${size} flex shrink-0 items-center justify-center text-[10px] font-semibold text-white ${user.kind === "agent" ? "rounded-md" : "rounded-full"}`}
      style={{ background: personColor(user.id) }}
      aria-hidden="true"
      title={user.kind === "agent" ? `${user.name} (agent)` : user.name}
    >
      {user.name.trim().charAt(0).toUpperCase()}
    </span>
  );
}

export function LabelDot({ color }: { color: string }) {
  return (
    <span
      className="size-2 shrink-0 rounded-full"
      style={{ background: readable(color) }}
      aria-hidden="true"
    />
  );
}

export function ProjectIcon({ className = "size-3.5" }: { className?: string }) {
  return (
    <svg viewBox="0 0 14 14" className={`${className} shrink-0`} aria-hidden="true" fill="none">
      <rect x="1.5" y="1.5" width="11" height="11" rx="3" stroke="var(--muted)" strokeWidth="1.3" />
      <path d="M4.5 7h5M7 4.5v5" stroke="var(--muted)" strokeWidth="1.3" />
    </svg>
  );
}

export function CheckIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      className="size-3.5"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      aria-hidden="true"
    >
      <path d="m3 8.5 3.5 3.5L13 4.5" />
    </svg>
  );
}

/** A service: two stacked boxes, like servers. */
/**
 * A service's own icon in its own colour, so it's recognisable wherever it
 * shows; a plain grey one for no service.
 */
export function ServiceIcon({
  service,
  className = "size-3.5",
}: {
  service?: { icon: string; color: string } | null;
  className?: string;
}) {
  const Glyph = ICONS[service?.icon ?? "server"] ?? IconServer;
  return (
    <Glyph
      aria-hidden="true"
      className={`${className} shrink-0`}
      // Its own colour, shaded where needed to stand out in this theme.
      color={service ? readable(service.color) : "var(--muted)"}
      stroke={2}
    />
  );
}

/** A page's icon, as the sidebar and the command menu show it. */
export function NavIcon({ icon: Icon }: { icon: LucideIcon }) {
  return <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />;
}
