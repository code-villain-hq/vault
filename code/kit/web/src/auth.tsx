// Sign-in screens every studio app shares: the company's account in the
// browser, and the packaged app's deep-link handoff. Each app names its
// default server and, when it has one, the "start on this Mac" copy.

import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { Button, buttonClass } from "./button";
import { AuthScreen, ErrorMessage, Splash, TextField } from "./controls";
import { isDesktop, onAppLink, openInBrowser } from "./desktop";
import {
  apiUrl,
  appConfig,
  authPlace,
  bundled,
  macUserName,
  normalizeServer,
  type Place,
  serverUrl,
  setServerUrl,
  startOnThisMac,
} from "./places";
import { messageOf } from "./problems";
import { useSignInProviders } from "./providers";
import { Redirect } from "./redirect";
import type { Session, SessionMe } from "./session";

/** What the auth pages need from the app's session. */
export type AuthSession<M extends SessionMe> = {
  useSession: () => Session<M>;
  signedIn: (me: M, place?: Place) => void;
  workspaceKey: (place: Place, workspaceId: string) => string | undefined;
};

/** Copy for `/start`. Apps without that screen (Layer) leave it out. */
export type AuthStartCopy = {
  workspacePlaceholder: string;
  offlineNote: string;
};

export type AuthPagesOptions = {
  /** The packaged app's suggested server address. */
  defaultServer: string;
  start?: AuthStartCopy;
};

/** Runs a form submission, tracking whether it's busy and what went wrong. */
function useSubmit(action: () => Promise<void>) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(messageOf(e));
      setBusy(false);
    }
  };
  return { busy, error, submit };
}

/** The server address someone typed, or "" when it doesn't look like one. */
function serverOrNothing(input: string) {
  try {
    return normalizeServer(input);
  } catch {
    return "";
  }
}

/** What went wrong signing in, by the code the server sends back. */
function oidcProblems(app: string, company: string): Record<string, string> {
  return {
    oidc_not_set_up: "Signing in isn't set up on this server.",
    oidc_expired: "That took too long. Try again.",
    oidc_other_browser: `Start signing in from ${app}, in this browser.`,
    oidc_refused: `${company} didn't sign you in. Try again.`,
    oidc_no_email: `Your ${company} account has no email address ${app} can use.`,
    oidc_no_access: `You don't have access to ${app} yet. Ask ${company}'s admin to add you.`,
    oidc_taken: `The ${app} account with your email is linked to another ${company} account.`,
    oidc_failed: "Something went wrong. Try again.",
  };
}

function oidcCode(code: string) {
  return Object.hasOwn(oidcProblems("", ""), code) ? code : "oidc_failed";
}

function problemOf(app: string, code: string, company?: string | null) {
  return oidcProblems(app, company || "Your company")[oidcCode(code)] ?? "Something went wrong.";
}

/** Only same-site paths, so a link can't send people elsewhere after signing in. */
function safeRedirect(path: string | undefined) {
  // Not "//host" or "/\host" (browsers read both as another site), nor
  // control characters.
  if (!path?.startsWith("/") || /^\/[/\\]/.test(path)) return "/";
  return [...path].some((c) => c.charCodeAt(0) < 32) ? "/" : path;
}

function hex(bytes: ArrayBuffer | Uint8Array) {
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Space, not rules, between the two ways in (tone, not lines). */
function Or() {
  return <p className="my-5 text-center text-xs text-faint">or</p>;
}

type AuthPages = {
  useProviders: (server?: string) => ReturnType<typeof useSignInProviders>;
  LoginPage: () => React.JSX.Element;
  SignedInPage: () => React.JSX.Element;
};

type AuthPagesWithStart = AuthPages & { StartPage: () => React.JSX.Element };

/** The app's session and options, set once by `createAuthPages`. */
let config: { session: AuthSession<SessionMe>; options: AuthPagesOptions } | undefined;

function pages() {
  if (!config) throw new Error("createAuthPages hasn't run: import the app's auth.tsx first.");
  return config;
}

function useProviders(server = serverUrl()) {
  return useSignInProviders(server);
}

function useServerChoice() {
  const name = appConfig().name;
  const { defaultServer } = pages().options;
  const [server, setServer] = useState(serverUrl() || defaultServer);
  // In the browser the app is on its server already: ask it ("").
  const [checked, setChecked] = useState(() => (bundled ? serverOrNothing(server) : ""));
  const field = bundled ? (
    <TextField
      label="Server"
      type="url"
      required
      placeholder={defaultServer}
      hint={`Where your team's ${name} runs.`}
      value={server}
      onChange={(e) => setServer(e.target.value)}
      onBlur={() => setChecked(serverOrNothing(server))}
    />
  ) : null;
  const apply = () => {
    if (!bundled) return;
    let url: string;
    try {
      url = normalizeServer(server);
    } catch {
      throw new Error("That server address doesn't look right.");
    }
    setServerUrl(url);
  };
  return { field, apply, checked };
}

type SignInProps = {
  label: string;
  company: string;
  query: Record<string, string>;
  before?: () => void;
};

function CompanyButton({ label, company, query, before }: SignInProps) {
  if (bundled)
    return <DesktopSignIn label={label} company={company} query={query} before={before} />;
  const params = new URLSearchParams(query).toString();
  return (
    <a
      href={`/api/auth/oidc${params ? `?${params}` : ""}`}
      className={`${buttonClass("primary")} flex`}
    >
      {label}
    </a>
  );
}

function useAfterSignIn() {
  const navigate = useNavigate();
  return useCallback(
    async (me: SessionMe, place: Place, redirect?: string) => {
      const asked = safeRedirect(redirect);
      const first = me.workspaces[0];
      const key = first && pages().session.workspaceKey(place, first.id);
      if (asked === "/" && key) await navigate({ to: "/$workspace", params: { workspace: key } });
      else await navigate({ to: asked });
    },
    [navigate],
  );
}

/**
 * The packaged app signs in in the browser, where passkeys and the
 * company's session are. The server sends back a one-time code through a
 * deep link, and only this app, which knows the secret behind the
 * challenge, can trade it for a session.
 */
function DesktopSignIn({ label, company, query, before }: SignInProps) {
  const afterSignIn = useAfterSignIn();
  const [secret, setSecret] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const app = appConfig();
  const scheme = `${app.key}://signed-in`;

  useEffect(() => {
    if (!secret) return;
    return onAppLink(async (url) => {
      if (!url.href.startsWith(scheme)) return;
      // The browser says why it didn't work; stop waiting and say so here.
      const problem = url.searchParams.get("error");
      if (problem) {
        setSecret(null);
        setError(problemOf(app.name, problem, company));
        return;
      }
      const code = url.searchParams.get("code");
      if (!code) return;
      setSecret(null);
      try {
        const place = authPlace();
        const me = await api<SessionMe>("POST", "/api/auth/oidc/desktop", { code, secret });
        pages().session.signedIn(me, place);
        await afterSignIn(me, place, url.searchParams.get("next") ?? undefined);
      } catch (e) {
        setError(messageOf(e));
      }
    });
  }, [secret, afterSignIn, company, app.name, scheme]);

  const start = async () => {
    setError(null);
    try {
      before?.();
      const fresh = hex(crypto.getRandomValues(new Uint8Array(32)));
      const challenge = hex(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(fresh)));
      setSecret(fresh);
      const params = new URLSearchParams({ ...query, desktop: challenge });
      await openInBrowser(apiUrl(`/api/auth/oidc?${params}`));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <>
      {error && (
        <div className="mb-4">
          <ErrorMessage>{error}</ErrorMessage>
        </div>
      )}
      <Button className="w-full" onClick={() => void start()}>
        {label}
      </Button>
      {secret && <p className="mt-3 text-center text-muted">Finish signing in in your browser.</p>}
    </>
  );
}

function BackLink() {
  if (!isDesktop) return null;
  return (
    <p className="mt-3 text-center text-muted">
      <Link to="/" className="ui-link">
        Back
      </Link>
    </p>
  );
}

function LoginPage() {
  const { useSession } = pages().session;
  const current = useSession();
  const { redirect, error: code } = useSearch({ strict: false }) as {
    redirect?: string;
    error?: string;
  };
  const server = useServerChoice();
  const providers = useProviders(server.checked);
  const name = appConfig().name;

  // The packaged app signs in to another server here, next to the ones it has.
  if (current.status === "signed-in" && !bundled) return <Redirect to={safeRedirect(redirect)} />;
  if (!providers) return <Splash />;
  const company = providers.oidc ? providers.company || "your company" : null;
  const problem = code ? problemOf(name, code, company) : null;
  const query: Record<string, string> = redirect ? { redirect: safeRedirect(redirect) } : {};

  return (
    <AuthScreen title={`Sign in to ${name}`}>
      {problem && (
        <div className="mb-4">
          <ErrorMessage>{problem}</ErrorMessage>
        </div>
      )}
      {server.field && <div className="mb-4">{server.field}</div>}
      {providers.error ? (
        <>
          <ErrorMessage>{providers.error}</ErrorMessage>
          <p className="mt-3 text-center text-muted">Waiting to reconnect…</p>
        </>
      ) : company ? (
        <>
          <CompanyButton
            label={`Sign in with ${company} ↗`}
            company={company}
            query={query}
            before={server.apply}
          />
          <p className="mt-6 text-center text-muted">New here? Ask {company}'s admin to add you.</p>
        </>
      ) : (
        // A desktop app's own server, or one whose sign-in isn't set up.
        <p className="text-center text-muted">
          {bundled && !server.checked
            ? `Enter the server your team's ${name} runs on.`
            : "This server doesn't sign anyone in."}
        </p>
      )}
      <BackLink />
    </AuthScreen>
  );
}

/**
 * Where a desktop sign-in ends in the browser: opens the desktop app with
 * the one-time code, and leaves a page that says the tab can go.
 */
function SignedInPage() {
  const { code, next, error } = useSearch({ from: "/signed-in" });
  const name = appConfig().name;
  const scheme = `${appConfig().key}://signed-in`;
  const problem = error ? oidcCode(error) : null;
  const link = problem
    ? `${scheme}?${new URLSearchParams({ error: problem })}`
    : /^[0-9a-f]{64}$/.test(code ?? "")
      ? `${scheme}?${new URLSearchParams({ code: code ?? "", next: safeRedirect(next) })}`
      : null;

  useEffect(() => {
    if (link) window.location.href = link;
  }, [link]);

  if (!link) return <Redirect to="/" />;
  if (problem) {
    return (
      <AuthScreen title="Couldn't sign you in">
        <div className="mb-4">
          <ErrorMessage>{problemOf(name, problem)}</ErrorMessage>
        </div>
        <p className="mb-6 text-center text-muted">
          {name} shows this too. You can close this tab.
        </p>
        <a href={link} className={`${buttonClass("secondary")} flex w-full`}>
          Back to {name}
        </a>
      </AuthScreen>
    );
  }
  return (
    <AuthScreen title="You're signed in">
      <p className="-mt-3 mb-6 text-center text-muted">
        {name} is opening. You can close this tab.
      </p>
      <a href={link} className={`${buttonClass("secondary")} flex w-full`}>
        Open {name}
      </a>
    </AuthScreen>
  );
}

/**
 * `/start`, the desktop app's first screen: start on this Mac, with no
 * server and no password, or sign in to one.
 */
function StartPage() {
  const { useSession, signedIn, workspaceKey } = pages().session;
  const startCopy = pages().options.start;
  const current = useSession();
  const navigate = useNavigate();
  const signedInHere = current.status === "signed-in" ? current.places : [];
  const started = signedInHere.find((s) => s.place.kind === "mac");
  const [name, setName] = useState("");
  const [workspace, setWorkspace] = useState("");
  const appName = appConfig().name;

  useEffect(() => {
    void macUserName().then((suggested) => setName((typed) => typed || suggested));
  }, []);

  const { busy, error, submit } = useSubmit(async () => {
    const before = new Set(started?.me.workspaces.map((w) => w.id));
    const place = await startOnThisMac(name, workspace);
    const me = await api<SessionMe>("GET", "/api/me", undefined, place);
    signedIn(me, place);
    const made = me.workspaces.find((w) => !before.has(w.id)) ?? me.workspaces[0];
    const key = made && workspaceKey(place, made.id);
    if (key) await navigate({ to: "/$workspace", params: { workspace: key } });
    else await navigate({ to: "/" });
  });

  // Only the desktop app starts on this Mac, and only an app with this screen.
  if (!isDesktop || !startCopy) return <Redirect to="/" />;
  if (current.status === "loading") return <Splash />;

  const workspaceField = (
    <TextField
      label="Workspace"
      required
      placeholder={startCopy.workspacePlaceholder}
      value={workspace}
      onChange={(e) => setWorkspace(e.target.value)}
    />
  );
  const cancel = signedInHere.length > 0 && (
    <p className="mt-6 text-center text-muted">
      <Link to="/" className="ui-link">
        Cancel
      </Link>
    </p>
  );

  if (started) {
    return (
      <AuthScreen title="New workspace on this Mac">
        <form onSubmit={submit} className="flex flex-col gap-4">
          {workspaceField}
          <ErrorMessage>{error}</ErrorMessage>
          <Button type="submit" disabled={busy}>
            {busy ? "Making…" : "Make workspace"}
          </Button>
        </form>
        {cancel}
      </AuthScreen>
    );
  }
  return (
    <AuthScreen title={`Start with ${appName}`}>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <TextField
          label="Your name"
          autoComplete="name"
          required
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        {workspaceField}
        <ErrorMessage>{error}</ErrorMessage>
        <Button type="submit" disabled={busy}>
          {busy ? "Starting…" : "Start on this Mac"}
        </Button>
        <p className="text-center text-xs text-faint">{startCopy.offlineNote}</p>
      </form>
      <Or />
      <Link to="/login" className={`${buttonClass("secondary")} flex`}>
        Sign in to a server
      </Link>
      {cancel}
    </AuthScreen>
  );
}

/**
 * The sign-in screens for one app. Call once at module level with that
 * app's session and its default server (and start-page copy when it has one).
 */
export function createAuthPages<M extends SessionMe>(
  session: AuthSession<M>,
  options: AuthPagesOptions & { start: AuthStartCopy },
): AuthPagesWithStart;
export function createAuthPages<M extends SessionMe>(
  session: AuthSession<M>,
  options: AuthPagesOptions & { start?: undefined },
): AuthPages;
export function createAuthPages<M extends SessionMe>(
  session: AuthSession<M>,
  options: AuthPagesOptions,
): AuthPages | AuthPagesWithStart {
  // The pages hand `signedIn` the app's own `M`: it's what its server sends.
  config = { session: session as unknown as AuthSession<SessionMe>, options };
  if (options.start) return { useProviders, LoginPage, SignedInPage, StartPage };
  return { useProviders, LoginPage, SignedInPage };
}
