// What ⌘K offers in Almanac, and the "G" keys, in the shared menus of
// `code/kit/web`: Calendar, the topics, a new topic, settings, workspaces,
// and what the topics keep.
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
  UserRound,
  UsersRound,
} from "lucide-react";
import { useMemo } from "react";
import { NavIcon, topicIcon } from "./icons";
import { useNewTopic, useTopics } from "./layout";
import { isDesktop } from "./platform";
import { plain } from "./prose";
import { placeName } from "./server";
import { signOut, useSession, workspaceEntries } from "./session";
import type { TopicSearch } from "./topic";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";

/** The command menu and the shortcuts list, for the workspace frame. */
export function AlmanacMenus() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const navigate = useNavigate();
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
        onSettings={
          isDesktop
            ? () =>
                void navigate({
                  to: guest ? "/$workspace/settings/account" : "/$workspace/settings/claude",
                  params: { workspace: workspace.url_key },
                })
            : undefined
        }
      />
    </>
  );
}

function useAlmanacCommands(query: string): Command[] {
  const { workspace, place } = useWorkspace();
  const guest = useIsGuest();
  const session = useSession();
  const navigate = useNavigate();
  const topics = useTopics();
  const newTopic = useNewTopic();
  const kept = useKept(query);

  const commands = useMemo(() => {
    const ws = workspace.url_key;
    const list: Command[] = [
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
    // A workspace on this Mac has no one to sign out.
    if (place.kind !== "mac") {
      list.push({
        id: "sign-out",
        group: "Account",
        label: place.kind === "server" ? `Sign out of ${placeName(place)}` : "Sign out",
        run: () =>
          void signOut(place).then((ok) =>
            ok
              ? navigate({ to: "/" })
              : navigate({
                  to: "/$workspace/settings/account",
                  params: { workspace: workspace.url_key },
                }),
          ),
      });
    }
    return list;
  }, [workspace, place, session, navigate, topics, guest, newTopic]);

  return useMemo(() => [...matching(commands, query), ...kept], [commands, query, kept]);
}

/** Something a topic keeps that ⌘K finds: its words, and the page it's on. */
interface Kept {
  id: string;
  icon: LucideIcon;
  /** What the row shows. */
  text: string;
  /** What a query is matched against, lowercased: the text, and a talk's messages. */
  words: string;
  open: () => void;
}

/**
 * What the topics keep, for ⌘K to find beyond their names: sections,
 * points, reminders and talks with every word of the query. Up to five a
 * topic, under its name, in the sidebar's order.
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
    const open = (topic: string, search: TopicSearch) => () =>
      void navigate({ to: "/$workspace/topics/$topic", params: { workspace: ws, topic }, search });
    const you = new Set(blocks.filter((b) => b.kind === "you").map((b) => b.id));
    const said = new Map<string, string>();
    for (const m of messages) said.set(m.talk_id, `${said.get(m.talk_id) ?? ""} ${m.text}`);
    const thing = (
      id: string,
      icon: LucideIcon,
      text: string,
      open: () => void,
      more = "",
    ): Kept => ({ id, icon, text, words: `${text} ${more}`.toLowerCase(), open });
    return topics.map((topic) => {
      const own = points.filter((p) => p.topic_id === topic.id);
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
        you.has(p.block_id) ? open(topic.id, { about: true }) : at(p.theme);
      return {
        topic,
        kept: [
          ...[...sections].map((theme) =>
            thing(`section-${topic.id}-${theme}`, FileText, theme, at(theme)),
          ),
          ...own.map((p) => thing(`point-${p.id}`, Dot, plain(p.text), of(p))),
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
    const words = query.split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    const found: Command[] = [];
    for (const { topic, kept: things } of kept) {
      let shown = 0;
      for (const k of things) {
        if (!words.every((word) => k.words.includes(word))) continue;
        found.push({
          id: k.id,
          group: `In ${topic.name}`,
          label: around(k.text, k.text.toLowerCase().indexOf(words[0] ?? "")),
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
