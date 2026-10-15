import { describe, expect, test } from "bun:test";
import { api } from "../src/lib/api";
import type { Settings } from "../src/lib/types";
import { useStore } from "../src/store";

// The backend, as far as these refreshes ask it.
const backend = api as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;

describe("a refresh that found nothing new", () => {
  test("keeps the settings object every settings view holds", async () => {
    const settings = { jira_connected: false, github_connected: false, github: null } as unknown as Settings;
    backend.getSettings = async () => structuredClone(settings);
    await useStore.getState().refreshSettings();
    const held = useStore.getState().settings;
    await useStore.getState().refreshSettings();
    expect(useStore.getState().settings).toBe(held);
  });

  test("after Sync or Clean up, keeps the projects and agents it already had", async () => {
    const projects = [{ id: "p", name: "web", path: "/tmp/web", default_branch: "main" }];
    const agents = [{ id: "claude", name: "Claude Code", program: "claude", installed: true }];
    backend.listProjects = async () => structuredClone(projects);
    backend.listTasks = async () => [];
    backend.listPanes = async () => [];
    backend.listAgents = async () => structuredClone(agents);
    backend.cursorIdeInstalled = async () => false;
    await useStore.getState().refreshAll();
    const before = useStore.getState();
    await useStore.getState().refreshAll();
    const after = useStore.getState();
    expect(after.projects).toBe(before.projects);
    expect(after.agents).toBe(before.agents);
    expect(after.tasks).toBe(before.tasks);
    expect(after.panes).toBe(before.panes);
  });
});
