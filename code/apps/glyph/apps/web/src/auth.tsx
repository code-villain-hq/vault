import { Redirect, useSignInProviders } from "@code-villain/web";
import type { Me } from "@glyph/types";
import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { isDesktop, onAppLink, openInBrowser } from "./platform";
import {
  apiUrl,
  authPlace,
  bundled,
  DEFAULT_SERVER,
  macUserName,
  normalizeServer,
  type Place,
  serverUrl,
  setServerUrl,
  startOnThisMac,
} from "./server";
import { signedIn, useSession, workspaceKey } from "./session";
import { AuthScreen, Button, buttonClass, ErrorMessage, Splash, TextField } from "./ui";

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

/**
 * The packaged desktop app asks which server to use; the browser app is
 * already on it. Returns the field, a function that applies the choice, and
 * the server whose ways in to show (checked when the field is left).
 */
function useServerChoice() {
  const [server, setServer] = useState(serverUrl() || DEFAULT_SERVER);
  // In the browser the app is on its server already: ask it ("").
  const [checked, setChecked] = useState(() => (bundled ? serverOrNothing(server) : ""));
  const field = bundled ? (
    <TextField
      label="Server"
      type="url"
      required
      placeholder={DEFAULT_SERVER}
      hint="Where your team's Glyph runs."
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

/** How this server signs people in: with its company's account, named on the button. */
export function useProviders(server = serverUrl()) {
  return useSignInProviders(server);
}

/** What went wrong signing in, by the code the server sends back, for a company's account. */
function oidcProblems(company: string): Record<string, string> {
  return {
    oidc_not_set_up: "Signing in isn't set up on this server.",
    oidc_expired: "That took too long. Try again.",
    oidc_other_browser: "Start signing in from Glyph, in this browser.",
    oidc_refused: `${company} didn't sign you in. Try again.`,
    oidc_no_email: `Your ${company} account has no email address Glyph can use.`,
    oidc_no_access: `You don't have access to Glyph yet. Ask ${company}'s admin to add you.`,
    oidc_taken: `The Glyph account with your email is linked to another ${company} account.`,
    oidc_failed: "Something went wrong. Try again.",
  };
}

/** A sign-in problem we have words for; unknown codes are the general one. */
function oidcCode(code: string) {
  return Object.hasOwn(oidcProblems(""), code) ? code : "oidc_failed";
}

/** Why signing in didn't work, for a code in the page's address. */
function problemOf(code: string, company?: string | null) {
  return oidcProblems(company || "Your company")[oidcCode(code)] ?? "Something went wrong.";
}

type SignInProps = {
  label: string;
  company: string;
  /** Where to go afterwards. */
  query: Record<string, string>;
  /** Runs first: the desktop app applies its choice of server. */
  before?: () => void;
};

/** Off to the company's sign-in; the server brings people back signed in. */
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

function hex(bytes: ArrayBuffer | Uint8Array) {
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * The packaged app signs in in the browser, where passkeys and the studio's
 * session are. The server sends back a one-time code through a
 * `glyph://signed-in` link, and only this app, which knows the secret behind
 * the challenge, can trade it for a session.
 */
function DesktopSignIn({ label, company, query, before }: SignInProps) {
  const afterSignIn = useAfterSignIn();
  const [secret, setSecret] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!secret) return;
    return onAppLink(async (url) => {
      if (!url.href.startsWith("glyph://signed-in")) return;
      // The browser says why it didn't work; stop waiting and say so here.
      const problem = url.searchParams.get("error");
      if (problem) {
        setSecret(null);
        setError(problemOf(problem, company));
        return;
      }
      const code = url.searchParams.get("code");
      if (!code) return;
      setSecret(null);
      try {
        const place = authPlace();
        const me = await api<Me>("POST", "/api/auth/oidc/desktop", { code, secret });
        signedIn(me, place);
        await afterSignIn(me, place, url.searchParams.get("next") ?? undefined);
      } catch (e) {
        setError(messageOf(e));
      }
    });
  }, [secret, afterSignIn, company]);

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

/** Space, not rules, between the two ways in (tone, not lines). */
function Or() {
  return <p className="my-5 text-center text-xs text-faint">or</p>;
}

/** Only same-site paths, so a link can't send people elsewhere after signing in. */
function safeRedirect(path: string | undefined) {
  // Not "//host" or "/\host" (browsers read both as another site), nor
  // control characters.
  if (!path?.startsWith("/") || /^\/[/\\]/.test(path)) return "/";
  return [...path].some((c) => c.charCodeAt(0) < 32) ? "/" : path;
}

/**
 * Where to go once signed in: where the link asked, or else the first
 * workspace in the place just signed in to (the desktop app may have others).
 */
function useAfterSignIn() {
  const navigate = useNavigate();
  return useCallback(
    async (me: Me, place: Place, redirect?: string) => {
      const asked = safeRedirect(redirect);
      const first = me.workspaces[0];
      const key = first && workspaceKey(place, first.id);
      if (asked === "/" && key) await navigate({ to: "/$workspace", params: { workspace: key } });
      else await navigate({ to: asked });
    },
    [navigate],
  );
}

/** The desktop app's sign-in screens lead back to its workspaces, or the start. */
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

export function LoginPage() {
  const session = useSession();
  const { redirect, error: code } = useSearch({ strict: false }) as {
    redirect?: string;
    error?: string;
  };
  const server = useServerChoice();
  const providers = useProviders(server.checked);

  // The packaged app signs in to another server here, next to the ones it has.
  if (session.status === "signed-in" && !bundled) return <Redirect to={safeRedirect(redirect)} />;
  if (!providers) return <Splash />;
  const company = providers.oidc ? providers.company || "your company" : null;
  const problem = code ? problemOf(code, company) : null;
  const query: Record<string, string> = redirect ? { redirect: safeRedirect(redirect) } : {};

  return (
    <AuthScreen title="Sign in to Glyph">
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
            ? "Enter the server your team's Glyph runs on."
            : "This server doesn't sign anyone in."}
        </p>
      )}
      <BackLink />
    </AuthScreen>
  );
}

/**
 * Where a desktop sign-in ends in the browser: opens the desktop app with
 * the one-time code, and leaves a page that says the tab can go. The code
 * is no use without the secret only that app holds. When signing in failed,
 * it says why, and tells the app so it stops waiting.
 */
export function SignedInPage() {
  const { code, next, error } = useSearch({ from: "/signed-in" });
  const problem = error ? oidcCode(error) : null;
  const link = problem
    ? `glyph://signed-in?${new URLSearchParams({ error: problem })}`
    : /^[0-9a-f]{64}$/.test(code ?? "")
      ? `glyph://signed-in?${new URLSearchParams({ code: code ?? "", next: safeRedirect(next) })}`
      : null;

  useEffect(() => {
    if (link) window.location.href = link;
  }, [link]);

  if (!link) return <Redirect to="/" />;
  if (problem) {
    return (
      <AuthScreen title="Couldn't sign you in">
        <div className="mb-4">
          <ErrorMessage>{problemOf(problem)}</ErrorMessage>
        </div>
        <p className="mb-6 text-center text-muted">Glyph shows this too. You can close this tab.</p>
        <a href={link} className={`${buttonClass("secondary")} flex w-full`}>
          Back to Glyph
        </a>
      </AuthScreen>
    );
  }
  return (
    <AuthScreen title="You're signed in">
      <p className="-mt-3 mb-6 text-center text-muted">Glyph is opening. You can close this tab.</p>
      <a href={link} className={`${buttonClass("secondary")} flex w-full`}>
        Open Glyph
      </a>
    </AuthScreen>
  );
}

/**
 * `/start`, the desktop app's first screen: start on this Mac, with no
 * server and no password, or sign in to one. Once started, it makes
 * another workspace on this Mac.
 */
export function StartPage() {
  const session = useSession();
  const navigate = useNavigate();
  const signedInHere = session.status === "signed-in" ? session.places : [];
  const started = signedInHere.find((s) => s.place.kind === "mac");
  const [name, setName] = useState("");
  const [workspace, setWorkspace] = useState("");

  useEffect(() => {
    void macUserName().then((suggested) => setName((typed) => typed || suggested));
  }, []);

  const { busy, error, submit } = useSubmit(async () => {
    const before = new Set(started?.me.workspaces.map((w) => w.id));
    const place = await startOnThisMac(name, workspace);
    const me = await api<Me>("GET", "/api/me", undefined, place);
    signedIn(me, place);
    const made = me.workspaces.find((w) => !before.has(w.id)) ?? me.workspaces[0];
    const key = made && workspaceKey(place, made.id);
    if (key) await navigate({ to: "/$workspace", params: { workspace: key } });
    else await navigate({ to: "/" });
  });

  if (!isDesktop) return <Redirect to="/" />;
  if (session.status === "loading") return <Splash />;

  const workspaceField = (
    <TextField
      label="Workspace"
      required
      placeholder="Villain Corp"
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
    <AuthScreen title="Start with Glyph">
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
        <p className="text-center text-xs text-faint">Works offline. Only you, on this Mac.</p>
      </form>
      <Or />
      <Link to="/login" className={`${buttonClass("secondary")} flex`}>
        Sign in to a server
      </Link>
      {cancel}
    </AuthScreen>
  );
}
