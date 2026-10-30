// Sample blocks for Settings → Blocks: every kind a page can have, with a
// few points each, drawn by the page's own components, and the one set of
// blocks, a collection once for each picture its fields pick. Nothing here
// syncs.

import type { Block, Facet, Field, Point, Reminder, Topic } from "@almanac/types";
import { addDays, today } from "./format";

/** A kind of block, as the gallery shows it: what it's called and for, and a page that has one. */
export interface Sample {
  name: string;
  says: string;
  topic: Topic;
  block: Block;
  points: Point[];
  reminders: Reminder[];
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
    starters: [],
    position: 0,
    kind: "topic",
  };
}

/** The samples, with their reminders due from today. */
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
    {
      facets = [],
      fields = [],
      reminders = [],
      width = "",
      view = "",
    }: {
      facets?: Facet[];
      fields?: Field[];
      reminders?: Reminder[];
      width?: string;
      view?: string;
    } = {},
  ): Sample => {
    const block: Block = {
      ...BASE,
      id: `sample-block-${kind}-${next()}`,
      topic_id: of.id,
      kind,
      title,
      facets,
      fields,
      width,
      view,
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
    return { name, says, topic: of, block, points: add(point), reminders };
  };

  const reminder = (of: Topic, title: string, due: number, every: number | null): Reminder => ({
    ...BASE,
    id: `sample-reminder-${next()}`,
    topic_id: of.id,
    theme: null,
    title,
    due_on: addDays(now, due),
    at: null,
    every_days: every,
    notify: false,
    status: "on",
    message_id: null,
    replaces: [],
    last_done_on: null,
    notified_for: null,
  });

  const running = topic("Running", "bike");
  const plants = topic("Plants", "sprout");
  const cat = topic("Cat", "cat");
  const history = topic("History", "globe");
  const career = topic("Career", "briefcase");
  const spanish = topic("Spanish", "languages");
  const brand = topic("Brand", "palette");
  const run = reminder(running, "Morning run", 0, 2);
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
      "Today",
      "The day's steps, under the routine's name, to tick off as you go.",
      running,
      "today",
      "Today",
      (p) => [
        p("Morning run", "Warm up: five minutes walking"),
        p("Morning run", "Run: 5 km at an easy pace"),
        p("Morning run", "Stretch: calves and hamstrings"),
      ],
      { reminders: [run] },
    ),
    sample(
      "Week",
      "The days things come round, once a talk settles a reminder. It keeps no points.",
      running,
      "week",
      "",
      () => [],
      { reminders: [run, reminder(running, "New shoes", 3, null)] },
    ),
    sample("Chips", "Short facts, a few words each.", spanish, "chips", "Terms", (p) => [
      p("", "Ser, for what something is"),
      p("", "Estar, for how it is now"),
      p("", "Por, for the way through"),
      p("", "Para, for where it's going"),
    ]),
    sample(
      "Lines",
      "Sentences that aren't steps, a line each, with where they're from. A colour code shows its colour.",
      brand,
      "lines",
      "Brand",
      (p) => [
        p("", "The accent is #a78bfa, for what you act on, never for surfaces"),
        p("", "Warn is #fbbf24 and danger #f87171, each only where something needs a look"),
        p("", "Headings stay in the text colour; weight sets them apart, not colour", {
          source: "Style guide",
        }),
      ],
    ),
    sample(
      "Shelf",
      "A tile per thing you use: what it is, how you use it, and its notes.",
      running,
      "shelf",
      "Shoes",
      (p) => [
        p("Pegasus 41", "Daily trainer"),
        p("Pegasus 41", "Easy runs and long runs, up to 20 km"),
        p("Pegasus 41", "Replace after about 700 km"),
        p("Vaporfly 3", "Race shoe"),
        p("Vaporfly 3", "Races and fast tempo runs only"),
      ],
    ),
    sample(
      "Cards",
      "A card per thing. The block names its facets, each with an icon; a card shows a line per facet, and what needs a look in warn or info.",
      plants,
      "cards",
      "Plants",
      (p) => [
        p("Ivy", "Variegated, cream edges"),
        p("Ivy", "Bright, no direct sun", { facet: "Light" }),
        p("Ivy", "When the top soil is dry", { facet: "Water" }),
        p("Ivy", "Shelf, the vines hang over the edge", { facet: "Place" }),
        p("Ivy", "Bare vines and brown leaves off", { facet: "Cutting" }),
        p("Calathea", "Prayer plant with dark spots"),
        p("Calathea", "Fungus gnats: the sticky trap stays until nothing sticks", {
          look: "warn",
        }),
        p("Calathea", "Bright, not right at the glass", { facet: "Light" }),
        p("Calathea", "Stale water, when the surface is dry", { facet: "Water" }),
        p("Monstera cutting", "In a glass of water"),
        p("Monstera cutting", "The roots are long enough: pot it", { look: "info" }),
        p("Monstera cutting", "Bathroom, grows slowly there", {
          facet: "Place",
          source: "You said",
        }),
        p("Monstera cutting", "Change the water once a week", { facet: "Water" }),
      ],
      {
        facets: [
          { name: "Light", icon: "sun" },
          { name: "Water", icon: "droplet" },
          { name: "Place", icon: "map-pin" },
          { name: "Cutting", icon: "scissors" },
        ],
      },
    ),
    sample(
      "Cards, with one card",
      "One card is a profile: its picture, what you'd say first as chips, and a tile per facet with its first lines.",
      cat,
      "cards",
      "Cat",
      (p) => [
        p("Lisa", "3½ years"),
        p("Lisa", "3.4 kg"),
        p("Lisa", "Forest cat mix, no papers"),
        p("Lisa", "Long-haired tabby, ruff and ear tufts"),
        p("Lisa", "Knows Felix wet food; dry comes as Heavenly Sterilised", { facet: "Food" }),
        p("Lisa", "Switch the wet food first, then the dry, one at a time", { facet: "Food" }),
        p("Lisa", "Won't eat Gourmet Gold", { facet: "Food" }),
        p("Lisa", "Leaves the bowl after an hour; nothing more until the next meal", {
          facet: "Food",
        }),
        p("Lisa", "Several times a day: the fishing rod, hide and seek", { facet: "Play" }),
        p("Lisa", "Sleeps 12–16 hours; her energy comes in the evening", { facet: "Play" }),
        p("Lisa", "Ribs you can feel, not see: keep her slim", { facet: "Health" }),
        p("Lisa", "The vet after diarrhoea or vomiting for more than a day or two", {
          facet: "Health",
        }),
      ],
      {
        facets: [
          { name: "Food", icon: "utensils" },
          { name: "Play", icon: "paw-print" },
          { name: "Health", icon: "heart-pulse" },
        ],
        reminders: [{ ...reminder(cat, "Weigh Lisa", 28, 30), theme: "Lisa" }],
      },
    ),
    sample(
      "Text",
      "Sections that explain, a tile each with its first sentence; the passages are on the section's page.",
      history,
      "text",
      "What it says",
      (p) => [
        p("Cuba", "**Cuba** was a Spanish colony until 1898. After that the US held the island."),
        p("Cuba", "The Platt Amendment let the US step in until 1934."),
        p(
          "Never colonised?",
          "**Ethiopia** stayed independent, apart from Italy's occupation from 1936 to 1941. Thailand did too.",
        ),
      ],
    ),
    sample(
      "List",
      "Things to follow or read, a line each, ticked off; what's done goes last.",
      history,
      "list",
      "Reading",
      (p) => [
        p("", "*Capital*, volume one", { source: "Karl Marx", done: true }),
        p("", "*The Wretched of the Earth*", { source: "Frantz Fanon" }),
        p("", "*Orientalism*", { source: "Edward Said" }),
      ],
    ),
    sample(
      "Timeline",
      "When each section happened, oldest first, a line each; its events are on its page.",
      history,
      "timeline",
      "Timeline",
      (p) => [
        p("Cuba", "The Ten Years' War begins", { happened: "1868" }),
        p("Cuba", "Spain leaves Cuba", { happened: "1898" }),
        p("Korea", "The Korean War", { happened: "1950–1953" }),
      ],
    ),
    sample(
      "Bars",
      "Numbers to compare: a set per theme, a bar per point on the set's own scale, a gap in warn.",
      career,
      "bars",
      "What the market wants",
      (p) => {
        const ads = { source: "28 ads, Stockholm and remote" };
        return [
          p("In 28 ads", "AI/LLM", { ...ads, amount: 25, note: "there, make it more visible" }),
          p("In 28 ads", "TypeScript", { ...ads, amount: 21, note: "in the CV" }),
          p("In 28 ads", "React", { ...ads, amount: 18, note: "in the CV" }),
          p("In 28 ads", "System design", {
            ...ads,
            amount: 14,
            note: "gap: notifications service",
            look: "warn",
          }),
          p("In 28 ads", "Testing", { ...ads, amount: 14, note: "1200+ end-to-end tests" }),
        ];
      },
    ),
    sample(
      "Table",
      "Things compared across the same facts: a row each, a column per facet. A header sorts, a cell changes where it is, New row starts one.",
      career,
      "table",
      "Applications",
      (p) => [
        p("Viaplay", "Senior Fullstack AI Engineer", { facet: "Role" }),
        p("Viaplay", "68 000 kr", { facet: "Salary", amount: 68000 }),
        p("Viaplay", "Second interview", { facet: "Stage", look: "info" }),
        p("Viaplay", "Hybrid, Stockholm", { facet: "Where" }),
        p("Schibsted", "Lead Engineer", { facet: "Role" }),
        p("Schibsted", "72 000 kr", { facet: "Salary", amount: 72000 }),
        p("Schibsted", "Applied", { facet: "Stage" }),
        p("Schibsted", "Office, three days", { facet: "Where" }),
        p("Neko Health", "Lead Software Engineer", { facet: "Role" }),
        p("Neko Health", "61 000 kr", { facet: "Salary", amount: 61000, look: "warn" }),
        p("Neko Health", "Take-home task", { facet: "Stage" }),
      ],
      {
        facets: [
          { name: "Role", icon: "briefcase" },
          { name: "Salary", icon: "wallet" },
          { name: "Stage", icon: "clock" },
          { name: "Where", icon: "map-pin" },
        ],
      },
    ),
    sample(
      "Trend",
      "A number kept each time it's measured, as a line by day: the latest to read at a glance, how it moved, and the mark it aims at, dashed.",
      cat,
      "trend",
      "Weight",
      (p) => [
        p("Lisa's weight", "At the vet", { happened: addDays(now, -130), amount: 3.6, note: "kg" }),
        p("Lisa's weight", "", { happened: addDays(now, -100), amount: 3.6, note: "kg" }),
        p("Lisa's weight", "", { happened: addDays(now, -65), amount: 3.5, note: "kg" }),
        p("Lisa's weight", "Less dry food since August", {
          happened: addDays(now, -34),
          amount: 3.5,
          note: "kg",
        }),
        p("Lisa's weight", "", { happened: addDays(now, -4), amount: 3.4, note: "kg" }),
        p("Lisa's weight", "Goal", { amount: 3.4, note: "kg" }),
      ],
      { reminders: [{ ...reminder(cat, "Weigh Lisa", 26, 30), theme: "Lisa's weight" }] },
    ),
    sample(
      "Parts",
      "A whole and the parts it's made of: a ring with the total, and each part's count and share, largest first. Pointing at one shows it in the middle.",
      career,
      "parts",
      "Where you'd work",
      (p) => [
        p("In 28 ads", "Hybrid, Stockholm", { amount: 12, note: "ads" }),
        p("In 28 ads", "Remote", { amount: 11 }),
        p("In 28 ads", "In the office", { amount: 5 }),
      ],
    ),
    sample(
      "Questions",
      "Questions you'd be asked, to practise: who asks, the answer folded under it, a tick once practised. Quiz me starts a talk that asks them one by one.",
      career,
      "questions",
      "Interview questions",
      (p) => [
        p(
          "How do you check AI output?",
          "After every agent run, tests, lint and typecheck say green or red.",
          {
            note: "Viaplay, Senior Fullstack AI Engineer",
            done: true,
          },
        ),
        p("How do you check AI output?", "A run is done only once its checks are green.", {
          done: true,
        }),
        p(
          "How do you measure whether LLM output is good enough?",
          "Evals: a fixed set of inputs with criteria, run on every change, giving a score.",
          { note: "Schibsted, Lead Engineer" },
        ),
        p("Explain a trade-off you made.", "No answer yet: ask a talk for one from your work.", {
          note: "Neko Health, Lead Software Engineer",
          look: "warn",
        }),
      ],
    ),
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
      "Bullets",
      "Short lines, a point each, in their order.",
      plants,
      "bullets",
      "Watering",
      (p) => [
        p("", "Water once the top 2 cm are dry"),
        p("", "Half as much from November to February"),
        p("", "Never leave it standing in water"),
      ],
    ),
    sample(
      "Checklist",
      "Things to do, a line each to tick, in their order. What's done goes faint and stays.",
      career,
      "checklist",
      "Before applying",
      (p) => [
        p("", "Bring the CV up to date", { done: true }),
        p("", "Write to two people who work there"),
        p("", "Read the last three posts on their blog"),
      ],
    ),
    sample(
      "Toggle",
      "A line for each part, which opens on what it keeps. Closed until you open it.",
      spanish,
      "toggle",
      "Grammar",
      (p) => [
        p("Ser and estar", "*Ser* for what something is: *es alta*"),
        p("Ser and estar", "*Estar* for how or where it is now: *está cansada*"),
        p("Por and para", "*Para* for a goal or a deadline: *para mañana*"),
        p("Por and para", "*Por* for a cause or a way through: *por el parque*"),
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
      "A name and two fields or more draw a table, a column per field.",
      plants,
      "collection",
      "Plants",
      (p) => [
        p("", "Monstera", {
          cells: { Light: "Bright, indirect", Water: "Weekly", "Safe for cats": false },
        }),
        p("", "Spider plant", {
          cells: { Light: "Bright", Water: "Weekly", "Safe for cats": true },
        }),
        p("", "Snake plant", { cells: { Light: "Any", Water: "Monthly", "Safe for cats": false } }),
      ],
      {
        fields: [field("Light", "choice"), field("Water", "text"), field("Safe for cats", "check")],
      },
    ),
    sample(
      "Collection: timeline",
      "Days or spans that are past draw a timeline, oldest first.",
      history,
      "collection",
      "Europe after Napoleon",
      (p) => [
        p("", "Revolutions across Europe", { cells: { When: "1848" } }),
        p("", "Congress of Vienna", { cells: { When: "1814–1815" } }),
        p("", "Unification of Germany", { cells: { When: "1871" } }),
      ],
      { fields: [field("When", "day")] },
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
      "Collection: list",
      "Anything else is a list: the name, then the rest.",
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
