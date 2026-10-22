import { Redirect } from "@code-villain/web";
import type { InvitePreview, Me } from "@raven/types";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
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
import { signedIn, signOut, useSession, workspaceKey } from "./session";
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
      hint="Where your team's Raven runs."
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

/** The ways in this server offers, and whether it still needs its first account. */
export type Providers = { oidc: boolean; password: boolean; setup: boolean };

/** Before the server answers, or when it can't: email and password. */
const PASSWORD_ONLY: Providers = { oidc: false, password: true, setup: false };

/**
 * How a server signs people in; null until it first says, so the page
 * doesn't flash the wrong way in. The packaged desktop app asks the server
 * chosen at sign-in.
 */
export function useProviders(server = serverUrl()) {
  const [providers, setProviders] = useState<Providers | null>(null);
  useEffect(() => {
    if (bundled && !server) {
      setProviders(PASSWORD_ONLY);
      return;
    }
    let current = true;
    fetch(`${server}/api/auth/providers`)
      .then((r) => (r.ok ? (r.json() as Promise<Providers>) : PASSWORD_ONLY))
      .catch(() => PASSWORD_ONLY)
      .then((p) => {
        if (current) setProviders(p);
      });
    return () => {
      current = false;
    };
  }, [server]);
  return providers;
}

/** What went wrong signing in with Code Villain, by the code the server sends back. */
const OIDC_PROBLEMS: Record<string, string> = {
  oidc_not_set_up: "Signing in with Code Villain isn't set up on this server.",
  oidc_expired: "That took too long. Try again.",
  oidc_other_browser: "Start signing in from Raven, in this browser.",
  oidc_refused: "Code Villain didn't sign you in. Try again.",
  oidc_no_email: "Your Code Villain account has no email address Raven can use.",
  oidc_no_account:
    "This Code Villain account has no Raven account yet. Ask your team for an invite link.",
  invite_gone: "That invite link has expired or was revoked. Ask for a new one.",
  oidc_taken: "The Raven account with your email is linked to another Code Villain account.",
  oidc_failed: "Something went wrong. Try again.",
};

/** A Code Villain sign-in problem we have words for; unknown codes are the general one. */
function oidcCode(code: string) {
  return Object.hasOwn(OIDC_PROBLEMS, code) ? code : "oidc_failed";
}

type SignInProps = {
  label: string;
  /** An invite to join with, or where to go afterwards. */
  query: Record<string, string>;
  /** Runs first: the desktop app applies its choice of server. */
  before?: () => void;
};

/** Off to the studio's sign-in; the server brings people back signed in. */
function CodeVillainButton({ label, query, before }: SignInProps) {
  if (bundled) return <DesktopSignIn label={label} query={query} before={before} />;
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
 * `raven://signed-in` link, and only this app, which knows the secret behind
 * the challenge, can trade it for a session.
 */
function DesktopSignIn({ label, query, before }: SignInProps) {
  const afterSignIn = useAfterSignIn();
  const [secret, setSecret] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!secret) return;
    return onAppLink(async (url) => {
      if (!url.href.startsWith("raven://signed-in")) return;
      // The browser says why it didn't work; stop waiting and say so here.
      const problem = url.searchParams.get("error");
      if (problem) {
        setSecret(null);
        setError(OIDC_PROBLEMS[oidcCode(problem)] ?? null);
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
  }, [secret, afterSignIn]);

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
  const { redirect, error: problemCode } = useSearch({ strict: false }) as {
    redirect?: string;
    error?: string;
  };
  const afterSignIn = useAfterSignIn();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const server = useServerChoice();
  const providers = useProviders(server.checked);
  const { busy, error, submit } = useSubmit(async () => {
    server.apply();
    const place = authPlace();
    const me = await api<Me>("POST", "/api/auth/login", { email, password });
    signedIn(me, place);
    await afterSignIn(me, place, redirect);
  });

  // The packaged app signs in to another server here, next to the ones it has.
  if (session.status === "signed-in" && !busy && !bundled) {
    return <Redirect to={safeRedirect(redirect)} />;
  }
  if (!providers) return <Splash />;
  const problem = problemCode ? OIDC_PROBLEMS[oidcCode(problemCode)] : null;
  if (providers.setup) return <SetupForm server={server} />;
  const query: Record<string, string> = redirect ? { redirect: safeRedirect(redirect) } : {};

  if (providers.oidc) {
    return (
      <AuthScreen title="Sign in to Raven">
        {problem && (
          <div className="mb-4">
            <ErrorMessage>{problem}</ErrorMessage>
          </div>
        )}
        {server.field && <div className="mb-4">{server.field}</div>}
        <CodeVillainButton
          label="Sign in with Code Villain ↗"
          query={query}
          before={server.apply}
        />
        <p className="mt-6 text-center text-muted">
          One account for every Code Villain app.
          <br />
          New here? Ask your team for an invite.
        </p>
        <BackLink />
      </AuthScreen>
    );
  }
  return (
    <AuthScreen title="Sign in to Raven">
      {problem && !error && (
        <div className="mb-4">
          <ErrorMessage>{problem}</ErrorMessage>
        </div>
      )}
      <form onSubmit={submit} className="flex flex-col gap-4">
        {server.field}
        <TextField
          label="Email"
          type="email"
          autoComplete="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <TextField
          label="Password"
          type="password"
          autoComplete="current-password"
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <ErrorMessage>{error}</ErrorMessage>
        <Button type="submit" disabled={busy}>
          {busy ? "Signing in…" : "Sign in"}
        </Button>
      </form>
      <p className="mt-6 text-center text-muted">New here? Ask your team for an invite.</p>
      <BackLink />
    </AuthScreen>
  );
}

/**
 * A Raven server without accounts asks for its first one, with the
 * workspace that watches Raven itself. After that, people join by invite.
 */
function SetupForm({ server }: { server: ReturnType<typeof useServerChoice> }) {
  const afterSignIn = useAfterSignIn();
  const [form, setForm] = useState({ name: "", email: "", password: "", workspace_name: "" });
  const field = (key: keyof typeof form) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  const { busy, error, submit } = useSubmit(async () => {
    server.apply();
    const place = authPlace();
    const me = await api<Me>("POST", "/api/auth/signup", form);
    signedIn(me, place);
    await afterSignIn(me, place);
  });

  return (
    <AuthScreen title="Set up Raven">
      <p className="-mt-3 mb-6 text-center text-muted">
        This Raven has no accounts yet. Yours is the first; everyone after you joins by invite.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        {server.field}
        <TextField label="Your name" autoComplete="name" required {...field("name")} />
        <TextField label="Email" type="email" autoComplete="email" required {...field("email")} />
        <TextField
          label="Password"
          type="password"
          autoComplete="new-password"
          required
          minLength={8}
          hint="At least 8 characters."
          {...field("password")}
        />
        <TextField
          label="Workspace name"
          required
          placeholder="Your team or company"
          {...field("workspace_name")}
        />
        <ErrorMessage>{error}</ErrorMessage>
        <Button type="submit" disabled={busy}>
          {busy ? "Setting up…" : "Set up Raven"}
        </Button>
      </form>
      <BackLink />
    </AuthScreen>
  );
}

/** Where an invite link lands: join as the signed-in user, or sign in or up first. */
export function JoinPage() {
  const { token } = useParams({ strict: false }) as { token: string };
  const session = useSession();
  const navigate = useNavigate();
  const [invite, setInvite] = useState<InvitePreview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [form, setForm] = useState({ name: "", email: "", password: "" });
  const providers = useProviders();
  // Invite links open in the browser, on the inviting server: "here".
  const here =
    session.status === "signed-in"
      ? session.places.find((s) => s.place.kind === "here")
      : undefined;

  useEffect(() => {
    api<InvitePreview>("GET", `/api/invites/${encodeURIComponent(token)}`)
      .then(setInvite)
      .catch((e) => setProblem(messageOf(e)));
  }, [token]);

  const open = async (me: Me) => {
    signedIn(me);
    await navigate({ to: "/$workspace", params: { workspace: invite?.workspace_url_key ?? "" } });
  };
  const accept = useSubmit(async () => {
    await open(await api<Me>("POST", `/api/invites/${encodeURIComponent(token)}/accept`));
  });
  const signup = useSubmit(async () => {
    await open(await api<Me>("POST", "/api/auth/signup", { ...form, invite_token: token }));
  });

  if (problem) {
    return (
      <AuthScreen title="This invite link doesn't work">
        <p className="text-center text-muted">
          {/\.$/.test(problem) ? problem : `${problem}.`} Ask for a new one.
        </p>
      </AuthScreen>
    );
  }
  if (!invite || !providers || session.status === "loading") return <Splash />;

  const title = `Join ${invite.workspace_name} on Raven`;
  const subtitle = (
    <p className="-mt-3 mb-6 text-center text-muted">
      {invite.invited_by} invited you as{" "}
      {{ admin: "an admin", member: "a member", guest: "a guest" }[invite.role]}.
    </p>
  );

  if (here) {
    return (
      <AuthScreen title={title}>
        {subtitle}
        <form onSubmit={accept.submit} className="flex flex-col gap-4">
          <ErrorMessage>{accept.error}</ErrorMessage>
          <Button type="submit" disabled={accept.busy}>
            Join as {here.me.user.name}
          </Button>
          <Button variant="secondary" onClick={() => void signOut(here.place)}>
            Not {here.me.user.name}? Sign out
          </Button>
        </form>
      </AuthScreen>
    );
  }

  if (providers.oidc) {
    return (
      <AuthScreen title={title}>
        {subtitle}
        <CodeVillainButton label="Sign in with Code Villain to join ↗" query={{ invite: token }} />
        <p className="mt-6 text-center text-muted">
          No Code Villain account yet? Ask {invite.invited_by} for one.
        </p>
      </AuthScreen>
    );
  }

  const field = (key: keyof typeof form) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  return (
    <AuthScreen title={title}>
      {subtitle}
      <form onSubmit={signup.submit} className="flex flex-col gap-4">
        <TextField label="Your name" autoComplete="name" required {...field("name")} />
        <TextField label="Email" type="email" autoComplete="email" required {...field("email")} />
        <TextField
          label="Password"
          type="password"
          autoComplete="new-password"
          required
          minLength={8}
          hint="At least 8 characters."
          {...field("password")}
        />
        <ErrorMessage>{signup.error}</ErrorMessage>
        <Button type="submit" disabled={signup.busy}>
          Create account and join
        </Button>
      </form>
      <p className="mt-6 text-center text-muted">
        Already have an account?{" "}
        <Link to="/login" search={{ redirect: `/join/${token}` }} className="ui-link">
          Sign in to join
        </Link>
      </p>
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
    ? `raven://signed-in?${new URLSearchParams({ error: problem })}`
    : /^[0-9a-f]{64}$/.test(code ?? "")
      ? `raven://signed-in?${new URLSearchParams({ code: code ?? "", next: safeRedirect(next) })}`
      : null;

  useEffect(() => {
    if (link) window.location.href = link;
  }, [link]);

  if (!link) return <Redirect to="/" />;
  if (problem) {
    return (
      <AuthScreen title="Couldn't sign you in">
        <div className="mb-4">
          <ErrorMessage>{OIDC_PROBLEMS[problem]}</ErrorMessage>
        </div>
        <p className="mb-6 text-center text-muted">Raven shows this too. You can close this tab.</p>
        <a href={link} className={`${buttonClass("secondary")} flex w-full`}>
          Back to Raven
        </a>
      </AuthScreen>
    );
  }
  return (
    <AuthScreen title="You're signed in">
      <p className="-mt-3 mb-6 text-center text-muted">Raven is opening. You can close this tab.</p>
      <a href={link} className={`${buttonClass("secondary")} flex w-full`}>
        Open Raven
      </a>
    </AuthScreen>
  );
}

function Or() {
  return <p className="my-5 text-center text-xs text-faint">or</p>;
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
      placeholder="My apps"
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
    <AuthScreen title="Start with Raven">
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
        <p className="text-center text-xs text-faint">
          Watches the apps on this Mac. Only you see it.
        </p>
      </form>
      <Or />
      <Link to="/login" className={`${buttonClass("secondary")} flex`}>
        Sign in to a server
      </Link>
      {cancel}
    </AuthScreen>
  );
}
