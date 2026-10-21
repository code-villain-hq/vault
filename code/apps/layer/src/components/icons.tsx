import {
  ArrowLeft, ArrowLeftRight, ArrowRight, ArrowUpRight, Bell, ChevronRight, Coffee, Copy, Globe,
  KeyRound, PanelLeft, Plus, Repeat, RotateCw, Settings, Smartphone, X,
  type LucideIcon,
} from "lucide-react";

/*
 * Icons are drawn rather than typed. A glyph (⚙, ✕, ▶, ⇄, +) takes its size
 * and weight from whatever font the fallback lands on: the ▶ chevrons were
 * 8px of ink, the ✕ on a pane tab thinner than the text beside it. A drawn
 * icon is the same size and stroke everywhere. It stays inline, so a button
 * holding only an icon is as tall as a labelled one beside it.
 *
 * They are Lucide's, the one set the studio's web apps use, at 15px and its
 * 1.75 stroke (studio/design/components.md, "Icon"). The names below are
 * what each means in Layer, so a screen asks for a gear, not for Settings.
 */

const STYLE = { verticalAlign: "middle", flex: "none" } as const;

function icon(Drawn: LucideIcon) {
  return function Icon({ size = 15 }: { size?: number }) {
    return <Drawn size={size} strokeWidth={1.75} style={STYLE} aria-hidden="true" />;
  };
}

export const GearIcon = icon(Settings);
/** A bell, for the message center. */
export const BellIcon = icon(Bell);
/** A cup, for keeping the Mac awake. */
export const CupIcon = icon(Coffee);
/** A phone, for phone access (PHONE-*). */
export const PhoneIcon = icon(Smartphone);
/** Points right; `.chev.open` turns it down. */
export const ChevronIcon = icon(ChevronRight);
export const CloseIcon = icon(X);
export const PlusIcon = icon(Plus);
/** Hand a pane's work to another agent. */
export const SwapIcon = icon(ArrowLeftRight);
export const ExternalIcon = icon(ArrowUpRight);
export const CopyIcon = icon(Copy);
/** A globe, for a task's browser. */
export const GlobeIcon = icon(Globe);
export const BackIcon = icon(ArrowLeft);
export const ForwardIcon = icon(ArrowRight);
export const ReloadIcon = icon(RotateCw);
/** A key, for a saved sign-in. */
export const KeyIcon = icon(KeyRound);
/** Two arrows chasing each other, for a loop (§21). */
export const LoopIcon = icon(Repeat);
/** The sidebar shown or hidden. */
export const PanelIcon = icon(PanelLeft);
