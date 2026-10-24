/**
 * Profiles (§25) for the mock harness: this window is Default, a second
 * profile is open in another window, a third is closed, and a fourth is on
 * layer.codevillain.eu. Opening one only marks it open; there is no second
 * window to show. Signing in to a server signs in at once, as Ada, with
 * Code Villain's account; localhost signs nobody in. With `?place=server`
 * this window is the server's workspace, signed out.
 */
import { emit } from "@tauri-apps/api/event";
import type { ProfileView, ServerSignedIn } from "../lib/types-profiles";

export function profileAnswers() {
  const profiles: ProfileView[] = [
    { id: "default", name: "ACME", open: true, current: true, worktree_root: "/Users/you/.villain-worktrees", colour: 0, server: null },
    { id: "personal", name: "Personal", open: true, current: false, worktree_root: "/Users/you/.villain-worktrees-personal", colour: 2, server: null },
    { id: "spare", name: "Spare", open: false, current: false, worktree_root: "/Users/you/.villain-worktrees-spare", colour: 4, server: null },
    { id: "code-villain", name: "Code Villain", open: false, current: false, worktree_root: "/Users/you/.villain-worktrees-code-villain", colour: 1, server: "layer.codevillain.eu" },
  ];
  // `?place=server`: this window is Code Villain's, on the server, signed out.
  if (new URLSearchParams(location.search).get("place") === "server") {
    for (const p of profiles) p.current = p.id === "code-villain";
    profiles[3].open = true;
  }
  const changed = () => void emit("profiles:changed");
  const find = (id: unknown) => {
    const p = profiles.find((x) => x.id === id);
    if (!p) throw new Error(`not found: profile ${String(id)}`);
    return p;
  };
  const taken = (name: string, except?: string) =>
    profiles.some((p) => p.id !== except && p.name.toLowerCase() === name.trim().toLowerCase());
  const workspaces: ServerSignedIn["workspaces"] = [
    { id: "0199a000-0000-7000-8000-000000000001", name: "Code Villain", url_key: "code-villain", role: "member" },
    { id: "0199a000-0000-7000-8000-000000000002", name: "Billing", url_key: "billing", role: "admin" },
  ];
  const signedIn = (address: string): ServerSignedIn => ({
    host: address.replace(/^https?:\/\//, "").replace(/\/.*$/, ""),
    company: "Code Villain",
    email: "ada@codevillain.eu",
    workspaces,
  });
  return {
    list_profiles: () => structuredClone(profiles),
    // The harness has no workspace server: the window works without one.
    workspace_place: () => null,
    read_setup_writes: () => [],
    ack_setup_writes: () => undefined,
    apply_setup: () => ({ applied: true, setup: false }),
    server_ways: (a: Record<string, unknown>) => {
      const address = String(a.address);
      if (!address.includes(".") && !address.startsWith("localhost")) throw new Error(`“${address}” is not a server address.`);
      const oidc = !address.startsWith("localhost");
      return { oidc, company: oidc ? "Code Villain" : null };
    },
    server_sign_in_browser: async (a: Record<string, unknown>) => {
      await new Promise((done) => setTimeout(done, 1500));
      return signedIn(String(a.address));
    },
    server_sign_in_cancel: () => null,
    server_open: (a: Record<string, unknown>) => {
      const workspace = workspaces.find((w) => w.id === a.id);
      if (!workspace) throw new Error(`not found: workspace ${String(a.id)}`);
      const known = profiles.find((p) => p.server === "layer.codevillain.eu" && p.name === workspace.name);
      if (known) known.open = true;
      else profiles.push({ id: workspace.url_key, name: workspace.name, open: true, current: false, worktree_root: `/Users/you/.villain-worktrees-${workspace.url_key}`, colour: 3, server: "layer.codevillain.eu" });
      changed();
      return null;
    },
    server_resume: () => null,
    open_members: () => null,
    open_profile: (a: Record<string, unknown>) => {
      find(a.id).open = true;
      return null;
    },
    create_profile: (a: Record<string, unknown>) => {
      const name = String(a.name).trim();
      if (!name || name.length > 40) throw new Error("A profile's name is 1 to 40 characters.");
      if (taken(name)) throw new Error(`There is a workspace called ${name} already.`);
      const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "profile";
      const made: ProfileView = { id, name, open: true, current: false, worktree_root: `/Users/you/.villain-worktrees-${id}`, colour: profiles.length % 6, server: null };
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
      if (taken(name, String(a.id))) throw new Error(`There is a workspace called ${name} already.`);
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
