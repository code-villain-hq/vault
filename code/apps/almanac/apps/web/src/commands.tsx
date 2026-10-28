// What ⌘K offers in Almanac, and the "G" keys, in the shared menus of
// `code/kit/web`: Calendar, the topics, a new topic, settings, workspaces.
import { type Command, CommandMenu, type GoTo, matching, Shortcuts } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import { CalendarDays, Plus, Sparkles, UserRound, UsersRound } from "lucide-react";
import { useMemo } from "react";
import { NavIcon, topicIcon } from "./icons";
import { useNewTopic, useTopics } from "./layout";
import { isDesktop } from "./platform";
import { placeName } from "./server";
import { signOut, useSession, workspaceEntries } from "./session";
import { useIsGuest, useWorkspace } from "./workspace";

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
        placeholder="Type a command, or find a topic…"
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

  return matching(commands, query);
}
