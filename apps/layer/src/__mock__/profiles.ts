/**
 * Profiles (§25) for the mock harness: this window is Default, a second
 * profile is open in another window, and a third is closed. Opening one only
 * marks it open; there is no second window to show.
 */
import { emit } from "@tauri-apps/api/event";
import type { ProfileView } from "../lib/types-profiles";

export function profileAnswers() {
  const profiles: ProfileView[] = [
    { id: "default", name: "ACME", open: true, current: true, worktree_root: "/Users/you/.villain-worktrees", colour: 0 },
    { id: "personal", name: "Personal", open: true, current: false, worktree_root: "/Users/you/.villain-worktrees-personal", colour: 2 },
    { id: "spare", name: "Spare", open: false, current: false, worktree_root: "/Users/you/.villain-worktrees-spare", colour: 4 },
  ];
  const changed = () => void emit("profiles:changed");
  const find = (id: unknown) => {
    const p = profiles.find((x) => x.id === id);
    if (!p) throw new Error(`not found: profile ${String(id)}`);
    return p;
  };
  const taken = (name: string, except?: string) =>
    profiles.some((p) => p.id !== except && p.name.toLowerCase() === name.trim().toLowerCase());
  return {
    list_profiles: () => structuredClone(profiles),
    open_profile: (a: Record<string, unknown>) => {
      find(a.id).open = true;
      return null;
    },
    create_profile: (a: Record<string, unknown>) => {
      const name = String(a.name).trim();
      if (!name || name.length > 40) throw new Error("A profile's name is 1 to 40 characters.");
      if (taken(name)) throw new Error(`There is a profile called ${name} already.`);
      const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "profile";
      const made: ProfileView = { id, name, open: true, current: false, worktree_root: `/Users/you/.villain-worktrees-${id}`, colour: profiles.length % 6 };
      profiles.push(made);
      changed();
      return structuredClone(made);
    },
    set_profile_colour: (a: Record<string, unknown>) => {
      find(a.id).colour = Number(a.colour);
      changed();
      return null;
    },
    rename_profile: (a: Record<string, unknown>) => {
      const name = String(a.name).trim();
      if (taken(name, String(a.id))) throw new Error(`There is a profile called ${name} already.`);
      find(a.id).name = name;
      changed();
      return null;
    },
    delete_profile: (a: Record<string, unknown>) => {
      const p = find(a.id);
      if (p.id === "default" || p.current || p.open) throw new Error(`${p.name} cannot be deleted now.`);
      profiles.splice(profiles.indexOf(p), 1);
      changed();
      return null;
    },
  };
}
