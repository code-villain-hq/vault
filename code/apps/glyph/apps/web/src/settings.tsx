import { type AccessConfig, accountPages } from "@code-villain/web";
import { useMemo } from "react";
import { api } from "./api";
import { languageName, useLanguages } from "./data";
import { PageHeader } from "./layout";
import { loadSession, signedIn, signOut } from "./session";
import { useWorkspace } from "./workspace";

/** The languages a guest may translate: every one but the source, which they all read. */
function useLanguageOptions() {
  const languages = useLanguages();
  return useMemo(
    () =>
      languages.filter((l) => !l.is_source).map((l) => ({ id: l.id, name: languageName(l.code) })),
    [languages],
  );
}

export const LANGUAGE_ACCESS: AccessConfig = {
  noun: "language",
  plural: "languages",
  useOptions: useLanguageOptions,
};

export const { MembersPage, AccountPage } = accountPages({
  name: "Glyph",
  api,
  useWorkspace,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
  access: LANGUAGE_ACCESS,
});
