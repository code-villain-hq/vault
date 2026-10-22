import { type AccessConfig, accountPages } from "@code-villain/web";
import { useMemo } from "react";
import { api } from "./api";
import { PageHeader } from "./layout";
import { useRepositories } from "./repositories";
import { loadSession, signedIn, signOut } from "./session";
import { useWorkspace } from "./workspace";

/** The repositories a guest may work in. */
function useRepositoryOptions() {
  const repositories = useRepositories();
  return useMemo(() => repositories.map((r) => ({ id: r.id, name: r.name })), [repositories]);
}

export const REPOSITORY_ACCESS: AccessConfig = {
  noun: "repository",
  plural: "repositories",
  useOptions: useRepositoryOptions,
};

export const { MembersPage, AccountPage } = accountPages({
  name: "Layer",
  api,
  useWorkspace,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
  access: REPOSITORY_ACCESS,
});
