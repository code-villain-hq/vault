// What ⌘K offers in Raven, and the "G" keys, in the shared menus of
// `code/kit/web`: the open app's views, the apps, settings, workspaces.
import { type Command, CommandMenu, type GoTo, matching, Shortcuts } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import { Activity, HeartPulse, LayoutGrid, UserRound, UsersRound } from "lucide-react";
import { useMemo } from "react";
import { Initial, NavIcon } from "./icons";
import { useAppKey, useApps } from "./layout";
import { isDesktop } from "./platform";
import { placeName } from "./server";
import { signOut, useSession, workspaceEntries } from "./session";
import { useIsGuest, useWorkspace } from "./workspace";

/** The command menu and the shortcuts list, for the workspace frame. */
export function RavenMenus() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const navigate = useNavigate();
  const app = useAppKey();
  // "G" then a letter goes to a view of the open app.
  const goTo: GoTo[] = app
    ? [
        { key: "h", name: "Health", to: "/$workspace/$app" },
        { key: "t", name: "Technical", to: "/$workspace/$app/technical" },
      ].map(({ key, name, to }) => ({
        key,
        name,
        go: () => void navigate({ to, params: { workspace: workspace.url_key, app } }),
      }))
    : [];
  return (
    <>
      <CommandMenu useCommands={useRavenCommands} placeholder="Type a command, or find an app…" />
      <Shortcuts
        goTo={goTo}
        onSettings={
          isDesktop
            ? () =>
                void navigate({
                  to: guest ? "/$workspace/settings/account" : "/$workspace/settings/apps",
                  params: { workspace: workspace.url_key },
                })
            : undefined
        }
      />
    </>
  );
}

function useRavenCommands(query: string): Command[] {
  const { workspace, place } = useWorkspace();
  const guest = useIsGuest();
  const session = useSession();
  const navigate = useNavigate();
  const apps = useApps();
  const open = useAppKey();

  const commands = useMemo(() => {
    const ws = workspace.url_key;
    const list: Command[] = [];
    if (open) {
      list.push(
        {
          id: "health",
          group: "Go to",
          label: "Health",
          hint: "G H",
          icon: <NavIcon icon={HeartPulse} />,
          run: () =>
            void navigate({ to: "/$workspace/$app", params: { workspace: ws, app: open } }),
        },
        {
          id: "technical",
          group: "Go to",
          label: "Technical",
          hint: "G T",
          icon: <NavIcon icon={Activity} />,
          run: () =>
            void navigate({
              to: "/$workspace/$app/technical",
              params: { workspace: ws, app: open },
            }),
        },
      );
    }
    list.push(
      ...apps.map((app) => ({
        id: `app-${app.id}`,
        group: "Apps",
        label: app.name,
        icon: <Initial name={app.name} square />,
        run: () =>
          void navigate({ to: "/$workspace/$app", params: { workspace: ws, app: app.key } }),
      })),
      ...(
        [
          ["apps", "Apps", LayoutGrid, "/$workspace/settings/apps"],
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
        run: () => void signOut(place).then(() => navigate({ to: "/" })),
      });
    }
    return list;
  }, [workspace, place, session, navigate, apps, open, guest]);

  return matching(commands, query);
}
