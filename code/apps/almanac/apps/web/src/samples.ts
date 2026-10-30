// Sample blocks for Settings → Blocks: every kind a page can have, with a
// few points each, drawn by the page's own components. Nothing here syncs.

import type { Block, Facet, Point, Reminder, Topic } from "@almanac/types";
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
      reminders = [],
      width = "",
    }: { facets?: Facet[]; reminders?: Reminder[]; width?: string } = {},
  ): Sample => {
    const block: Block = {
      ...BASE,
      id: `sample-block-${kind}-${next()}`,
      topic_id: of.id,
      kind,
      title,
      facets,
      width,
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
  ];
}
