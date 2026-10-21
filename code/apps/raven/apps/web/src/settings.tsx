import { accountPages } from "@code-villain/web";
import { api } from "./api";
import { useProviders } from "./auth";
import { PageHeader } from "./layout";
import { publicUrl } from "./server";
import { loadSession, signedIn, signOut } from "./session";
import { useWorkspace } from "./workspace";

export const { MembersPage, AccountPage } = accountPages({
  name: "Raven",
  api,
  useWorkspace,
  usePasswords: (origin) => useProviders(origin)?.password,
  publicUrl,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
});
