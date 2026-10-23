// What ⌘K offers in Glyph, and the "G" keys, in the shared menus of
// `code/kit/web`: the views, settings, workspaces.
import { type Command, CommandMenu, type GoTo, matching, Shortcuts } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import {
  ArrowLeftRight,
  BookA,
  Cpu,
  Gauge,
  Globe,
  Languages,
  ListChecks,
  UserRound,
  UsersRound,
} from "lucide-react";
import { useMemo } from "react";
import { NavIcon } from "./icons";
import { isDesktop } from "./platform";
import { placeName } from "./server";
import { signOut, useSession, workspaceEntries } from "./session";
import { useIsGuest, useWorkspace } from "./workspace";

/** The views, each with its "G" key, in the sidebar's order. */
const VIEWS = [
  { key: "o", name: "Overview", icon: Gauge, to: "/$workspace" },
  { key: "t", name: "Translations", icon: Languages, to: "/$workspace/translations" },
  { key: "r", name: "Review", icon: ListChecks, to: "/$workspace/review" },
  { key: "l", name: "Glossary", icon: BookA, to: "/$workspace/glossary" },
  { key: "i", name: "Import & export", icon: ArrowLeftRight, to: "/$workspace/import" },
] as const;

const SETTINGS = [
  { id: "languages", name: "Languages", icon: Globe, to: "/$workspace/settings/languages" },
  { id: "models", name: "Models", icon: Cpu, to: "/$workspace/settings/models" },
  { id: "members", name: "Members", icon: UsersRound, to: "/$workspace/settings/members" },
  { id: "account", name: "Account", icon: UserRound, to: "/$workspace/settings/account" },
] as const;

/** The command menu and the shortcuts list, for the workspace frame. */
export function GlyphMenus() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const navigate = useNavigate();
  const params = { workspace: workspace.url_key };
  const goTo: GoTo[] = VIEWS.map(({ key, name, to }) => ({
    key,
    name,
    go: () => void navigate({ to, params }),
  }));
  return (
    <>
      <CommandMenu useCommands={useGlyphCommands} placeholder="Type a command, or find a view…" />
      <Shortcuts
        goTo={goTo}
        onSettings={
          isDesktop
            ? () =>
                void navigate({
                  to: guest ? "/$workspace/settings/account" : "/$workspace/settings/languages",
                  params,
                })
            : undefined
        }
      />
    </>
  );
}

function useGlyphCommands(query: string): Command[] {
  const { workspace, place } = useWorkspace();
  const guest = useIsGuest();
  const session = useSession();
  const navigate = useNavigate();

  const commands = useMemo(() => {
    const params = { workspace: workspace.url_key };
    const list: Command[] = [
      ...VIEWS.map(({ key, name, icon, to }) => ({
        id: `view-${key}`,
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
  }, [workspace, place, session, navigate, guest]);

  return matching(commands, query);
}
