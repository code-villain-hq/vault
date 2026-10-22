// What ⌘K offers in Layer's web app, and the "G" keys, in the shared menus
// of `code/kit/web`: what the workspace shares, settings, workspaces.
import { type Command, CommandMenu, type GoTo, matching, Shortcuts } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import { FolderGit2, Plug, UserRound, UsersRound } from "lucide-react";
import { useMemo } from "react";
import { NavIcon } from "./icons";
import { isDesktop } from "./platform";
import { placeName } from "./server";
import { signOut, useSession, workspaceEntries } from "./session";
import { useIsGuest, useWorkspace } from "./workspace";

/** The pages, each with its "G" key, in the sidebar's order. */
const PAGES = [
  { key: "r", name: "Repositories", icon: FolderGit2, to: "/$workspace/repositories" },
  { key: "c", name: "Connections", icon: Plug, to: "/$workspace/connections" },
] as const;

const SETTINGS = [
  { id: "members", name: "Members", icon: UsersRound, to: "/$workspace/settings/members" },
  { id: "account", name: "Account", icon: UserRound, to: "/$workspace/settings/account" },
] as const;

/** The command menu and the shortcuts list, for the workspace frame. */
export function LayerMenus() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const navigate = useNavigate();
  const params = { workspace: workspace.url_key };
  const goTo: GoTo[] = PAGES.map(({ key, name, to }) => ({
    key,
    name,
    go: () => void navigate({ to, params }),
  }));
  return (
    <>
      <CommandMenu useCommands={useLayerCommands} placeholder="Type a command, or find a page…" />
      <Shortcuts
        goTo={goTo}
        onSettings={
          isDesktop
            ? () =>
                void navigate({
                  to: guest ? "/$workspace/settings/account" : "/$workspace/settings/members",
                  params,
                })
            : undefined
        }
      />
    </>
  );
}

function useLayerCommands(query: string): Command[] {
  const { workspace, place } = useWorkspace();
  const guest = useIsGuest();
  const session = useSession();
  const navigate = useNavigate();

  const commands = useMemo(() => {
    const params = { workspace: workspace.url_key };
    const list: Command[] = [
      ...PAGES.map(({ key, name, icon, to }) => ({
        id: `page-${key}`,
        group: "Go to",
        label: name,
        hint: `G ${key.toUpperCase()}`,
        icon: <NavIcon icon={icon} />,
        run: () => void navigate({ to, params }),
      })),
      // A guest has only their account to set.
      ...SETTINGS.filter((s) => !guest || s.id === "account").map(({ id, name, icon, to }) => ({
        id: `settings-${id}`,
        group: "Settings",
        label: name,
        icon: <NavIcon icon={icon} />,
        run: () => void navigate({ to, params }),
      })),
      ...(session.status === "signed-in" ? workspaceEntries(session.places) : [])
        .filter((e) => e.key !== workspace.url_key)
        .map((e) => ({
          id: `workspace-${e.key}`,
          group: "Workspaces",
          label: `Switch to ${e.workspace.name}`,
          run: () => void navigate({ to: "/$workspace", params: { workspace: e.key } }),
        })),
    ];
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
  }, [workspace, place, session, navigate, guest]);

  return matching(commands, query);
}
