// What ⌘K offers in Almanac, and the "G" keys, in the shared menus of
// `code/kit/web`: on a topic's page a block to add, Calendar, the topics,
// a new topic, settings, workspaces, and what the topics keep.
import type { Point } from "@almanac/types";
import { type Command, CommandMenu, type GoTo, matching, Shortcuts } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import {
  Bell,
  CalendarDays,
  Dot,
  FileText,
  type LucideIcon,
  MessagesSquare,
  Plus,
  Sparkles,
  Table2,
  UserRound,
  UsersRound,
} from "lucide-react";
import { useMemo } from "react";
import { fold } from "./format";
import { NavIcon, topicIcon } from "./icons";
import { useNewTopic, useOpenSettings, useSignOut, useTopics } from "./layout";
import { isDesktop } from "./platform";
import { plain } from "./prose";
import { useSession, workspaceEntries } from "./session";
import type { TopicSearch } from "./topic";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";
import { ADDS, useAdder } from "./writing";

/** The command menu and the shortcuts list, for the workspace frame. */
export function AlmanacMenus() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const navigate = useNavigate();
  const openSettings = useOpenSettings();
  const goTo: GoTo[] = [
    {
      key: "c",
      name: "Calendar",
      go: () =>
        void navigate({ to: "/$workspace/calendar", params: { workspace: workspace.url_key } }),
    },
  ];
  return (
    <>
      <CommandMenu
        useCommands={useAlmanacCommands}
        placeholder="Type a command, or search your topics…"
      />
      <Shortcuts
        goTo={goTo}
        anywhere={guest ? [] : [["C", "New topic"]]}
        sections={[
          {
            title: "On a topic",
            keys: [
              ["/", "Ask the topic"],
              ["Esc", "Close the talk"],
            ],
          },
        ]}
        onSettings={isDesktop ? openSettings : undefined}
      />
    </>
  );
}

function useAlmanacCommands(query: string): Command[] {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const session = useSession();
  const navigate = useNavigate();
  const topics = useTopics();
  const newTopic = useNewTopic();
  const kept = useKept(query);
  const out = useSignOut();
  const add = useAdder();

  const commands = useMemo(() => {
    const ws = workspace.url_key;
    // On a topic's page, first: a block of the set at its end.
    const adds: Command[] =
      add && !guest
        ? ADDS.flat().map(({ kind, name, icon }) => ({
            id: `add-${kind}`,
            group: "This page",
            label: `Add a ${name.toLowerCase()}`,
            icon: <NavIcon icon={icon} />,
            run: () => add(kind),
          }))
        : [];
    const list: Command[] = [
      ...adds,
      {
        id: "calendar",
        group: "Go to",
        label: "Calendar",
        hint: "G C",
        icon: <NavIcon icon={CalendarDays} />,
        run: () => void navigate({ to: "/$workspace/calendar", params: { workspace: ws } }),
      },
    ];
    if (!guest) {
      list.push({
        id: "new-topic",
        group: "Topics",
        label: "New topic",
        hint: "C",
        icon: <NavIcon icon={Plus} />,
        run: () => newTopic(),
      });
    }
    list.push(
      ...topics.map((topic) => ({
        id: `topic-${topic.id}`,
        group: "Topics",
        label: topic.name,
        icon: <NavIcon icon={topicIcon(topic.icon)} />,
        run: () =>
          void navigate({
            to: "/$workspace/topics/$topic",
            params: { workspace: ws, topic: topic.id },
          }),
      })),
      ...(
        [
          ["claude", "Claude", Sparkles, "/$workspace/settings/claude"],
          ["members", "Members", UsersRound, "/$workspace/settings/members"],
          ["account", "Account", UserRound, "/$workspace/settings/account"],
        ] as const
      )
        // A guest has only their account to set.
        .filter(([id]) => !guest || id === "account")
        .map(([id, label, icon, to]) => ({
          id: `settings-${id}`,
          group: "Settings",
          label,
          icon: <NavIcon icon={icon} />,
          run: () => void navigate({ to, params: { workspace: ws } }),
        })),
      ...(session.status === "signed-in" ? workspaceEntries(session.places) : [])
        .filter((e) => e.key !== ws)
        .map((e) => ({
          id: `workspace-${e.key}`,
          group: "Workspaces",
          label: `Switch to ${e.workspace.name}`,
          run: () => void navigate({ to: "/$workspace", params: { workspace: e.key } }),
        })),
    );
    if (out) list.push({ id: "sign-out", group: "Account", label: out.label, run: out.run });
    return list;
  }, [workspace, session, navigate, topics, guest, newTopic, out, add]);

  return useMemo(() => [...matching(commands, query), ...kept], [commands, query, kept]);
}

/** Something a topic keeps that ⌘K finds: its words, and the page it's on. */
interface Kept {
  id: string;
  icon: LucideIcon;
  /** What the row shows. */
  text: string;
  /** What a query is matched against, folded: the text, a row's cells, a talk's messages. */
  words: string;
  open: () => void;
}

/**
 * What the topics keep, for ⌘K to find beyond their names: sections,
 * blocks by their titles, points with their cells, reminders and talks
 * with every word of the query. Up to five a topic, under its name, in the
 * sidebar's order.
 */
function useKept(query: string): Command[] {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const topics = useTopics();
  const points = useRecords("Point");
  const blocks = useRecords("Block");
  const reminders = useRecords("Reminder");
  const talks = useRecords("Talk");
  const messages = useRecords("Message");

  // Lowercased once per change of the records rather than on every key, as
  // Forge's menu does: answers are long.
  const kept = useMemo(() => {
    const ws = workspace.url_key;
    const open = (topic: string, search: TopicSearch, hash?: string) => () =>
      void navigate({
        to: "/$workspace/topics/$topic",
        params: { workspace: ws, topic },
        search,
        hash,
      });
    const you = new Set(blocks.filter((b) => b.kind === "you").map((b) => b.id));
    const said = new Map<string, string>();
    for (const m of messages) said.set(m.talk_id, `${said.get(m.talk_id) ?? ""} ${m.text}`);
    const thing = (
      id: string,
      icon: LucideIcon,
      text: string,
      open: () => void,
      more = "",
    ): Kept => ({ id, icon, text, words: fold(`${text} ${more}`), open });
    return topics.map((topic) => {
      const own = points.filter((p) => p.topic_id === topic.id);
      const byId = new Map(own.map((p) => [p.id, p]));
      const its = blocks.filter((b) => b.topic_id === topic.id);
      // The row whose page a block sits on, while the row is there.
      const rowOf = new Map(
        its.flatMap((b) => {
          const row = b.row_id ? byId.get(b.row_id) : undefined;
          return row ? [[b.id, row] as const] : [];
        }),
      );
      // A hit on a block scrolls to it, on the page or on its row's.
      const toBlock = (id: string) => {
        const row = rowOf.get(id);
        return open(topic.id, row ? { section: row.text, row: row.id } : {}, `block-${id}`);
      };
      const shown = new Set(its.filter((b) => !b.row_id).map((b) => b.id));
      const due = reminders.filter(
        (r) => r.topic_id === topic.id && (r.status === "on" || r.status === "done"),
      );
      const sections = new Set(
        [...own.map((p) => p.theme), ...due.map((r) => r.theme ?? "")].filter(Boolean),
      );
      // A point in no section opens the page;
      // what the topic knows about you opens its About.
      const at = (theme: string) => open(topic.id, theme ? { section: theme } : {});
      const of = (p: Point) =>
        you.has(p.block_id)
          ? open(topic.id, { about: true })
          : rowOf.has(p.block_id) || (!p.theme && shown.has(p.block_id))
            ? toBlock(p.block_id)
            : at(p.theme);
      return {
        topic,
        kept: [
          ...[...sections].map((theme) =>
            thing(`section-${topic.id}-${theme}`, FileText, theme, at(theme)),
          ),
          ...its
            .filter((b) => b.title && (!b.row_id || rowOf.has(b.id)))
            .map((b) => thing(`block-${b.id}`, Table2, b.title, toBlock(b.id))),
          ...own.map((p) =>
            thing(
              `point-${p.id}`,
              Dot,
              plain(p.text),
              of(p),
              Object.values(p.cells)
                .filter((c) => typeof c !== "boolean")
                .join(" "),
            ),
          ),
          ...due.map((r) => thing(`reminder-${r.id}`, Bell, r.title, at(r.theme ?? ""))),
          ...talks
            .filter((t) => t.topic_id === topic.id)
            .map((t) =>
              thing(
                `talk-${t.id}`,
                MessagesSquare,
                t.title || "Talk",
                open(topic.id, { talk: t.id }),
                said.get(t.id),
              ),
            ),
        ],
      };
    });
  }, [workspace, navigate, topics, points, blocks, reminders, talks, messages]);

  return useMemo(() => {
    const words = fold(query).split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    const found: Command[] = [];
    for (const { topic, kept: things } of kept) {
      let shown = 0;
      for (const k of things) {
        if (!words.every((word) => k.words.includes(word))) continue;
        found.push({
          id: k.id,
          group: `In ${topic.name}`,
          label: around(k.text, fold(k.text).indexOf(words[0] ?? "")),
          icon: <NavIcon icon={k.icon} />,
          run: k.open,
        });
        if (++shown === 5) break;
      }
    }
    return found;
  }, [kept, query]);
}

/** The text from a little before `at`, so a match deep in a long point shows. */
function around(text: string, at: number): string {
  if (at < 32) return text;
  const start = text.lastIndexOf(" ", at - 16);
  return start > 0 ? `…${text.slice(start + 1)}` : text;
}
