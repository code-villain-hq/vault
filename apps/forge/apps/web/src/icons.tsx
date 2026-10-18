import type { StateType, WorkflowState } from "@forge/types";
import { IconServer } from "@tabler/icons-react";
import {
  Box,
  Check,
  Circle,
  CircleArrowRight,
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleX,
  type LucideIcon,
  Minus,
  SignalHigh,
  SignalLow,
  SignalMedium,
  SquareExclamationPoint,
} from "lucide-react";
import { readable } from "./contrast";
import { useReviewStateIds } from "./data";
import { ICONS } from "./icon-set";

/** Lucide's size and stroke: 15px beside text, 12px inside chips and pills. */
type IconSize = 12 | 15;
const STROKE = 1.75;

/** A workflow state's icon, telling its team's review status apart. */
export function StateIcon({
  state,
  color,
  size,
}: {
  state: WorkflowState;
  color?: string;
  size?: IconSize;
}) {
  const review = useReviewStateIds().has(state.id);
  return <StatusIcon type={state.type} color={color ?? state.color} review={review} size={size} />;
}

/** Lucide has no half-full circle, so "started" is drawn on its grid and stroke. */
function Started({ color, size }: { color: string; size: IconSize }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      className="shrink-0"
      aria-hidden="true"
      fill="none"
      stroke={color}
      strokeWidth={STROKE}
    >
      <circle cx="12" cy="12" r="10" />
      <path d="M12 6a6 6 0 0 1 0 12z" fill={color} stroke="none" />
    </svg>
  );
}

const STATE_ICONS: Record<Exclude<StateType, "started">, LucideIcon> = {
  triage: CircleArrowRight,
  backlog: CircleDashed,
  unstarted: Circle,
  completed: CircleCheck,
  canceled: CircleX,
};

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
  size = 15,
}: {
  type: StateType;
  color: string;
  review?: boolean;
  size?: IconSize;
}) {
  // The team's colour, adjusted where needed to stand out from the background.
  const color = readable(chosen);
  if (type === "started" && !review) return <Started color={color} size={size} />;
  const Icon = review ? CircleDot : STATE_ICONS[type as Exclude<StateType, "started">];
  return (
    <Icon size={size} strokeWidth={STROKE} color={color} aria-hidden="true" className="shrink-0" />
  );
}

/** 0 = none, 1 = urgent, 2 = high, 3 = medium, 4 = low. */
export function PriorityIcon({ priority, size = 15 }: { priority: number; size?: IconSize }) {
  const [Icon, color] =
    priority === 1
      ? [SquareExclamationPoint, "var(--status-danger-ink)"]
      : priority === 2
        ? [SignalHigh, "var(--muted)"]
        : priority === 3
          ? [SignalMedium, "var(--muted)"]
          : priority === 4
            ? [SignalLow, "var(--muted)"]
            : [Minus, "var(--faint)"];
  return (
    <Icon size={size} strokeWidth={STROKE} color={color} aria-hidden="true" className="shrink-0" />
  );
}

/**
 * A neutral initial on the chip tint, in the control role: round for a
 * person, square for an agent or a workspace. Grey does the work; a
 * person's avatar is detail, not a signal.
 */
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

/** A person's initial (a square for agents); dashed for nobody. */
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
    <Initial
      name={user.name}
      square={user.kind === "agent"}
      size={size}
      title={user.kind === "agent" ? `${user.name} (agent)` : user.name}
    />
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

export function ProjectIcon({ size = 15 }: { size?: IconSize }) {
  return (
    <Box size={size} strokeWidth={STROKE} aria-hidden="true" className="shrink-0 text-muted" />
  );
}

export function CheckIcon() {
  return <Check size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />;
}

/**
 * A service's own icon in its own colour, so it's recognisable wherever it
 * shows; a plain grey one for no service.
 */
export function ServiceIcon({
  service,
  className = "size-[15px]",
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
      stroke={1.75}
    />
  );
}

/** A page's icon, as the sidebar and the command menu show it. */
export function NavIcon({ icon: Icon }: { icon: LucideIcon }) {
  return <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />;
}
