import type { ReactNode } from "react";

/*
 * Icons are drawn rather than typed. A glyph (⚙, ✕, ▶, ⇄, +) takes its size
 * and weight from whatever font the fallback lands on: the ▶ chevrons were
 * 8px of ink, the ✕ on a pane tab thinner than the text beside it. A drawn
 * icon is the same size and stroke everywhere. It stays inline, so a button
 * holding only an icon is as tall as a labelled one beside it.
 *
 * The paths are Lucide's (ISC), the one set the studio's web apps draw from
 * (design/components.md), at its 1.75 stroke.
 */

function Svg({ size, stroke = 1.75, children }: { size: number; stroke?: number; children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ verticalAlign: "middle", flex: "none" }}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function GearIcon({ size = 20 }: { size?: number }) {
  return (
    <Svg size={size}>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9v.09a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </Svg>
  );
}

/** A bell, for the message center. */
export function BellIcon({ size = 20 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" />
      <path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />
    </Svg>
  );
}

/** A cup, for keeping the Mac awake. */
export function CupIcon({ size = 20 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M4 9h13v5a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5z" />
      <path d="M17 11h1.5a2.5 2.5 0 0 1 0 5H17" />
      <path d="M8 3.5v2.5M12 3.5v2.5" />
    </Svg>
  );
}

/** A phone, for phone access (PHONE-*). */
export function PhoneIcon({ size = 20 }: { size?: number }) {
  return (
    <Svg size={size}>
      <rect x="6.5" y="2.5" width="11" height="19" rx="2.5" />
      <path d="M11 18.5h2" />
    </Svg>
  );
}

/** Points right; `.chev.open` turns it down. */
export function ChevronIcon({ size = 12 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2.6}>
      <path d="M9 5l7 7-7 7" />
    </Svg>
  );
}

export function CloseIcon({ size = 14 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2.2}>
      <path d="M6 6l12 12M18 6L6 18" />
    </Svg>
  );
}

export function PlusIcon({ size = 16 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2.2}>
      <path d="M12 5v14M5 12h14" />
    </Svg>
  );
}

/** Hand a pane's work to another agent. */
export function SwapIcon({ size = 14 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2}>
      <path d="M4 8h15l-4-4M20 16H5l4 4" />
    </Svg>
  );
}

export function ExternalIcon({ size = 14 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2}>
      <path d="M8 16L17 7M9 7h8v8" />
    </Svg>
  );
}

export function CopyIcon({ size = 14 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2}>
      <rect x="8" y="8" width="12" height="12" rx="2" />
      <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
    </Svg>
  );
}

/** A globe, for a task's browser. */
export function GlobeIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3z" />
    </Svg>
  );
}

export function BackIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2}>
      <path d="M19 12H5M11 6l-6 6 6 6" />
    </Svg>
  );
}

export function ForwardIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2}>
      <path d="M5 12h14M13 6l6 6-6 6" />
    </Svg>
  );
}

export function ReloadIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size} stroke={2}>
      <path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" />
    </Svg>
  );
}

/** A key, for a saved sign-in. */
export function KeyIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <circle cx="8" cy="15" r="4" />
      <path d="M11 12l9-9M17 6l3 3M14 9l2 2" />
    </Svg>
  );
}

/** Two arrows chasing each other, for a loop (§21). */
export function LoopIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M4 12a8 8 0 0 1 13.7-5.6L20 9M20 4v5h-5" />
      <path d="M20 12a8 8 0 0 1-13.7 5.6L4 15M4 20v-5h5" />
    </Svg>
  );
}

/** The views in the sidebar: Work, Tickets, Reviews, Chat, Usage, Repos. */
export function WorkIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="m7 11 2-2-2-2" />
      <path d="M11 13h4" />
      <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
    </Svg>
  );
}

export function TicketIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M2 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z" />
      <path d="M13 5v2M13 17v2M13 11v2" />
    </Svg>
  );
}

export function ReviewIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <circle cx="18" cy="18" r="3" />
      <circle cx="6" cy="6" r="3" />
      <path d="M13 6h3a2 2 0 0 1 2 2v7" />
      <path d="M6 9v12" />
    </Svg>
  );
}

export function ChatIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
    </Svg>
  );
}

export function UsageIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M3 3v16a2 2 0 0 0 2 2h16" />
      <path d="M18 17V9M13 17V5M8 17v-3" />
    </Svg>
  );
}

export function RepoIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <path d="M9 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v5" />
      <circle cx="13" cy="12" r="2" />
      <path d="M18 19c-2.8 0-5-2.2-5-5v8" />
      <circle cx="20" cy="19" r="2" />
    </Svg>
  );
}

/** The sidebar shown or hidden. */
export function PanelIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size}>
      <rect width="18" height="18" x="3" y="3" rx="2" />
      <path d="M9 3v18" />
    </Svg>
  );
}
