// Sample blocks for Settings → Blocks: every kind of the one set a page is
// made of, with a few points each, drawn by the page's own components, and a
// collection once for each picture its fields pick. Nothing here syncs.

import type { Block, Field, Point, Topic } from "@almanac/types";
import { addDays, today } from "./format";

/** A kind of block, as the gallery shows it: what it's called and for, and a page that has one. */
export interface Sample {
  name: string;
  says: string;
  topic: Topic;
  block: Block;
  points: Point[];
}

const AT = "2026-10-01T09:00:00Z";
const BASE = { workspace_id: "sample", created_at: AT, updated_at: AT };

function topic(name: string, icon: string): Topic {
  return {
    ...BASE,
    id: `sample-${name}`,
    name,
    sticks_to: "",
    icon,
    group_name: "",
    parent_id: null,
    cover: null,
    starters: [],
    position: 0,
    kind: "topic",
  };
}

/** The samples, their days counted from today. */
export function samples(): Sample[] {
  let n = 0;
  const next = () => {
    n += 1;
    return n;
  };
  const now = today();

  /** A block of the sample, with the points `add` gives it. */
  const sample = (
    name: string,
    says: string,
    of: Topic,
    kind: string,
    title: string,
    add: (point: (theme: string, text: string, more?: Partial<Point>) => Point) => Point[],
    { fields = [], view = "" }: { fields?: Field[]; view?: string } = {},
  ): Sample => {
    const block: Block = {
      ...BASE,
      id: `sample-block-${kind}-${next()}`,
      topic_id: of.id,
      kind,
      title,
      facets: [],
      fields,
      width: "",
      view,
      row_id: null,
      position: 0,
    };
    const point = (theme: string, text: string, more: Partial<Point> = {}): Point => ({
      ...BASE,
      id: `sample-point-${next()}`,
      topic_id: of.id,
      block_id: block.id,
      theme,
      text,
      happened: "",
      facet: "",
      look: "",
      amount: null,
      note: "",
      done: false,
      cells: {},
      source: null,
      source_url: null,
      talk_id: null,
      seen: true,
      position: n,
      ...more,
    });
    return { name, says, topic: of, block, points: add(point) };
  };

  const running = topic("Running", "bike");
  const plants = topic("Plants", "sprout");
  const cat = topic("Cat", "cat");
  const history = topic("History", "globe");
  const career = topic("Career", "briefcase");
  const spanish = topic("Spanish", "languages");
  const field = (name: string, kind: string, more: Partial<Field> = {}): Field => ({
    name,
    kind,
    unit: "",
    goal: null,
    options: [],
    ...more,
  });

  return [
    sample(
      "You",
      "What you tell it about yourself, which its answers go by. The topic's About shows it, not its page.",
      career,
      "you",
      "You",
      (p) => [
        p("", "Lives in Stockholm, works remote"),
        p("", "Ten years of TypeScript, React for eight"),
      ],
    ),
    sample(
      "Heading",
      "A title that starts a part of the page, for the blocks under it. It keeps no points.",
      cat,
      "heading",
      "Care",
      () => [],
    ),
    sample(
      "Paragraph",
      "Writing that explains, a paragraph a point, at a measure that reads well. Where it's from shows once, under it.",
      history,
      "paragraph",
      "Why it lasted",
      (p) => [
        p(
          "",
          "The Congress of Vienna drew borders that held, more or less, for a century: no great power left the table wanting another war.",
          {
            source: "Britannica",
            source_url: "https://www.britannica.com/event/Congress-of-Vienna",
          },
        ),
        p(
          "",
          "It met again whenever a crisis came up, so the great powers talked before they fought.",
          {
            source: "Britannica",
            source_url: "https://www.britannica.com/event/Congress-of-Vienna",
          },
        ),
      ],
    ),
    sample(
      "Paragraph: leads",
      "When each paragraph opens with a bold lead, they're cards: the lead, a few lines, and More opens one across.",
      history,
      "paragraph",
      "How it held",
      (p) => [
        p(
          "",
          "**Borders** drawn in Vienna held, more or less, for a century: no great power left the table wanting another war, and the small ones were told where they stood.",
        ),
        p("", "**Talks** came before fights: the powers met again whenever a crisis came up."),
        p(
          "",
          "**Revolutions** in 1848 shook it, but the order held until the unification of Germany and Italy redrew the map.",
        ),
      ],
    ),
    sample(
      "Callout",
      "What to keep in mind, in a box that stands out, with a warning sign when it's one.",
      cat,
      "callout",
      "",
      (p) => [p("", "Lilies are poisonous to cats, even the pollen.", { look: "warn" })],
    ),
    sample(
      "Collection: line",
      "A collection is rows with fields, and its fields pick its picture, the same on every page. A day with a number draws a line, the goal dashed.",
      cat,
      "collection",
      "Weight",
      (p) => [
        p("", "Weighing", { cells: { Day: addDays(now, -65), Weight: 3.6 } }),
        p("", "Weighing", { cells: { Day: addDays(now, -34), Weight: 3.5 } }),
        p("", "Weighing", { cells: { Day: addDays(now, -4), Weight: 3.4 } }),
      ],
      { fields: [field("Day", "day"), field("Weight", "number", { unit: "kg", goal: 3.3 })] },
    ),
    sample(
      "Collection: board",
      "A status draws a board: a lane per step, the first still open and the last done.",
      career,
      "collection",
      "Applications",
      (p) => [
        p("", "Northwind", { cells: { Status: "Applied", Role: "Lead engineer" } }),
        p("", "Globex", { cells: { Status: "To apply", Role: "Staff engineer" } }),
        p("", "Initech", { cells: { Status: "Interview", Role: "Senior engineer" } }),
        p("", "Umbrella", { cells: { Status: "To apply", Role: "Engineering manager" } }),
      ],
      {
        fields: [
          field("Status", "status", { options: ["To apply", "Applied", "Interview", "Offer"] }),
          field("Role", "text"),
        ],
      },
    ),
    sample(
      "Collection: cards",
      "A photo draws cards, a card per row. Any collection can be viewed as cards.",
      plants,
      "collection",
      "Plants",
      (p) => [
        p("", "Monstera", { cells: { Light: "Bright, indirect", Water: "Weekly" } }),
        p("", "Snake plant", { cells: { Light: "Any", Water: "Monthly" } }),
        p("", "Calathea", { cells: { Light: "Shade", Water: "Twice a week" } }),
      ],
      { fields: [field("Light", "choice"), field("Water", "text")], view: "cards" },
    ),
    sample(
      "Collection: table",
      "A name and two fields or more draw a table, a column per field, a number with a bar on its column's largest.",
      plants,
      "collection",
      "Plants",
      (p) => [
        p("", "Monstera", {
          cells: {
            Light: "Bright, indirect",
            Water: "Weekly",
            Height: 120,
            "Safe for cats": false,
          },
        }),
        p("", "Spider plant", {
          cells: { Light: "Bright", Water: "Weekly", Height: 30, "Safe for cats": true },
        }),
        p("", "Snake plant", {
          cells: { Light: "Any", Water: "Monthly", Height: 70, "Safe for cats": false },
        }),
      ],
      {
        fields: [
          field("Light", "choice"),
          field("Water", "text"),
          field("Height", "number", { unit: "cm" }),
          field("Safe for cats", "check"),
        ],
      },
    ),
    sample(
      "Collection: figures",
      "A few names, each with a short value, draw figures: a tile each, the value large. A colour written #a78bfa shows as its swatch.",
      cat,
      "collection",
      "At a glance",
      (p) => [
        p("", "Age", { cells: { Value: "3½ years" } }),
        p("", "Weight", { cells: { Value: "3,4 kg" } }),
        p("", "Sleep", { cells: { Value: "12–16 h a day" } }),
      ],
      { fields: [field("Value", "text")] },
    ),
    sample(
      "Collection: steps",
      "Rows in their order can be viewed as steps, numbered, the next one marked.",
      cat,
      "collection",
      "Changing food",
      (p) => [
        p("", "Keep the old food as the base"),
        p("", "Mix in a spoon of the new"),
        p("", "A little more each week"),
      ],
      { view: "steps" },
    ),
    sample(
      "Collection: timeline",
      "Days or spans that are past draw a timeline, oldest first, with an axis over them from six on: a lane per choice, which a click picks; past ten rows the list shows the first few, with All.",
      history,
      "collection",
      "Europe after Napoleon",
      (p) => [
        p("", "French Revolution", { cells: { When: "1789–1799", Where: "France" } }),
        p("", "Napoleon's wars", { cells: { When: "1803–1815", Where: "Europe" } }),
        p("", "Revolutions across Europe", { cells: { When: "1848", Where: "Europe" } }),
        p("", "Congress of Vienna", { cells: { When: "1814–1815", Where: "Europe" } }),
        p("", "July Revolution", { cells: { When: "1830", Where: "France" } }),
        p("", "Unification of Germany", { cells: { When: "1871", Where: "Germany" } }),
      ],
      { fields: [field("When", "day"), field("Where", "choice")] },
    ),
    sample(
      "Collection: week",
      "Days still to come draw a week from today, with what comes after it below.",
      running,
      "collection",
      "Runs",
      (p) => [
        p("", "Long run", { cells: { Day: addDays(now, 2) } }),
        p("", "Intervals", { cells: { Day: addDays(now, 4) } }),
        p("", "City 10 km", { cells: { Day: addDays(now, 12) } }),
      ],
      { fields: [field("Day", "day")] },
    ),
    sample(
      "Collection: bars",
      "A name with a number draws bars on one scale, the rest beside them.",
      running,
      "collection",
      "Shoes",
      (p) => [
        p("", "Pegasus 41", { cells: { Price: 89, Shop: "Runners Point" } }),
        p("", "Ghost 16", { cells: { Price: 119, Shop: "Sportscheck" } }),
        p("", "Clifton 9", { cells: { Price: 105 } }),
      ],
      { fields: [field("Price", "number", { unit: "€" }), field("Shop", "text")] },
    ),
    sample(
      "Collection: checklist",
      "A check, with a day at most besides, draws a checklist: a row a line to tick. What's done goes faint and stays.",
      career,
      "collection",
      "Before applying",
      (p) => [
        p("", "Bring the CV up to date", { cells: { Done: true } }),
        p("", "Write to two people who work there"),
        p("", "Read the last three posts on their blog"),
      ],
      { fields: [field("Done", "check")] },
    ),
    sample(
      "Collection: list",
      "Anything else is a list: the name, then the rest. Every row opens a page of its own, for what's said about it.",
      spanish,
      "collection",
      "Words",
      (p) => [
        p("", "madrugada", { cells: { Means: "the hours after midnight" } }),
        p("", "sobremesa", { cells: { Means: "staying at the table to talk" } }),
        p("", "estrenar", { cells: { Means: "to wear or use for the first time" } }),
      ],
      { fields: [field("Means", "text")] },
    ),
  ];
}
