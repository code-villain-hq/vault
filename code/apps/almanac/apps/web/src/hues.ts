// A topic's hue: one of the six data hues (`data-1` to `data-6`), by its
// group, so the topics of a group wear one colour. The page draws its bars,
// lines, steps and figures in it; a choice's options take the other five,
// in their order. Chrome stays grey.

import type { Topic } from "@almanac/types";
import type { CSSProperties } from "react";

/** How many data hues the design has. */
const HUES = 6;

/** The groups in the sidebar's order: the ungrouped topics first, then the groups by name. */
export function groupsOf(topics: Topic[]): string[] {
  const names = new Set(
    topics.filter((t) => t.kind !== "almanac" && !t.parent_id).map((t) => t.group_name),
  );
  return [...names].sort((a, b) => (a && b ? a.localeCompare(b) : a ? 1 : -1));
}

/** A topic's hue, 1 to 6: its group's, or its top topic's for a sub-topic. */
export function hueOf(topic: Topic, topics: Topic[]): number {
  let top = topic;
  for (let up = 0; top.parent_id && up < topics.length; up++) {
    const parent = topics.find((t) => t.id === top.parent_id);
    if (!parent) break;
    top = parent;
  }
  const at = groupsOf(topics).indexOf(top.group_name);
  return at < 0 ? 1 : (at % HUES) + 1;
}

/**
 * The colours a topic's page draws with, as CSS variables: `--hue`, its
 * own, and `--choice-1` to `--choice-5`, the other hues in their order.
 */
export function hueStyle(hue: number): CSSProperties {
  const others = Array.from({ length: HUES }, (_, n) => n + 1).filter((n) => n !== hue);
  return Object.fromEntries([
    ["--hue", `var(--data-${hue})`],
    ...others.map((n, i) => [`--choice-${i + 1}`, `var(--data-${n})`]),
  ]) as CSSProperties;
}
