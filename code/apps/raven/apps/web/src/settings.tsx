import { type AccessConfig, accountPages } from "@code-villain/web";
import { useMemo } from "react";
import { api } from "./api";
import { PageHeader, useApps } from "./layout";
import { loadSession, signedIn, signOut } from "./session";
import { useWorkspace } from "./workspace";

/** The apps a guest may watch. */
function useAppOptions() {
  const apps = useApps();
  return useMemo(() => apps.map((a) => ({ id: a.id, name: a.name })), [apps]);
}

export const APP_ACCESS: AccessConfig = { noun: "app", plural: "apps", useOptions: useAppOptions };

export const { MembersPage, AccountPage } = accountPages({
  name: "Raven",
  api,
  useWorkspace,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
  access: APP_ACCESS,
});
