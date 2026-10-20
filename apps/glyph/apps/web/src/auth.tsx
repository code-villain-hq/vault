import type { InvitePreview, Me } from "@glyph/types";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { type FormEvent, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { onAppLink, openInBrowser } from "./platform";
import { Redirect } from "./redirect";
import {
  apiUrl,
  bundled,
  DEFAULT_SERVER,
  normalizeServer,
  serverUrl,
  setServerUrl,
} from "./server";
import { signedIn, signOut, useSession } from "./session";
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

/** The ways in this server offers. */
export type Providers = { oidc: boolean; password: boolean };

/** Before the server answers, or when it can't: email and password. */
const PASSWORD_ONLY: Providers = { oidc: false, password: true };

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
  oidc_other_browser: "Start signing in from Glyph, in this browser.",
  oidc_refused: "Code Villain didn't sign you in. Try again.",
  oidc_no_email: "Your Code Villain account has no email address Glyph can use.",
  oidc_no_account:
    "This Code Villain account has no Glyph account yet. Ask your team for an invite link.",
  invite_gone: "That invite link has expired or was revoked. Ask for a new one.",
  oidc_taken: "The Glyph account with your email is linked to another Code Villain account.",
  oidc_failed: "Something went wrong. Try again.",
};

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
 * `glyph://signed-in` link, and only this app, which knows the secret behind
 * the challenge, can trade it for a session.
 */
function DesktopSignIn({ label, query, before }: SignInProps) {
  const navigate = useNavigate();
  const [secret, setSecret] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!secret) return;
    return onAppLink(async (url) => {
      const code = url.searchParams.get("code");
      if (!url.href.startsWith("glyph://signed-in") || !code) return;
      setSecret(null);
      try {
        signedIn(await api<Me>("POST", "/api/auth/oidc/desktop", { code, secret }));
        await navigate({ to: safeRedirect(url.searchParams.get("next") ?? undefined) });
      } catch (e) {
        setError(messageOf(e));
      }
    });
  }, [secret, navigate]);

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

export function LoginPage() {
  const session = useSession();
  const { redirect, error: problemCode } = useSearch({ strict: false }) as {
    redirect?: string;
    error?: string;
  };
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const server = useServerChoice();
  const providers = useProviders(server.checked);
  const { busy, error, submit } = useSubmit(async () => {
    server.apply();
    signedIn(await api<Me>("POST", "/api/auth/login", { email, password }));
    await navigate({ to: safeRedirect(redirect) });
  });

  if (session.status === "signed-in" && !busy) {
    return <Redirect to={safeRedirect(redirect)} />;
  }
  if (!providers) return <Splash />;
  const problem = problemCode ? (OIDC_PROBLEMS[problemCode] ?? OIDC_PROBLEMS.oidc_failed) : null;
  const query: Record<string, string> = redirect ? { redirect: safeRedirect(redirect) } : {};

  if (providers.oidc) {
    return (
      <AuthScreen title="Sign in to Glyph">
        {problem && (
          <div className="mb-4">
            <ErrorMessage>{problem}</ErrorMessage>
          </div>
        )}
        {server.field && <div className="mb-4">{server.field}</div>}
        <CodeVillainButton label="Sign in with Code Villain" query={query} before={server.apply} />
        <p className="mt-6 text-center text-muted">
          One account for every Code Villain app.
          <br />
          New here? Ask your team for an invite.
        </p>
      </AuthScreen>
    );
  }
  return (
    <AuthScreen title="Sign in to Glyph">
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
      <p className="mt-6 text-center text-muted">
        New to Glyph?{" "}
        <Link to="/signup" className="ui-link">
          Create an account
        </Link>
      </p>
    </AuthScreen>
  );
}

export function SignupPage() {
  const session = useSession();
  const navigate = useNavigate();
  const [form, setForm] = useState({ name: "", email: "", password: "", workspace_name: "" });
  const field = (key: keyof typeof form) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  const server = useServerChoice();
  const providers = useProviders(server.checked);
  const { busy, error, submit } = useSubmit(async () => {
    server.apply();
    const me = await api<Me>("POST", "/api/auth/signup", form);
    signedIn(me);
    const workspace = me.workspaces[0];
    await navigate(
      workspace ? { to: "/$workspace", params: { workspace: workspace.url_key } } : { to: "/" },
    );
  });

  if (session.status === "signed-in" && !busy) return <Redirect to="/" />;
  if (!providers) return <Splash />;
  // With Code Villain accounts, people come in through invites only.
  if (providers.oidc) return <Redirect to="/login" />;
  return (
    <AuthScreen title="Create your Glyph account">
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
          {busy ? "Creating…" : "Create account"}
        </Button>
      </form>
      <p className="mt-6 text-center text-muted">
        Already have an account?{" "}
        <Link to="/login" className="ui-link">
          Sign in
        </Link>
      </p>
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

  const title = `Join ${invite.workspace_name} on Glyph`;
  const subtitle = (
    <p className="-mt-3 mb-6 text-center text-muted">
      {invite.invited_by} invited you as {invite.role === "admin" ? "an admin" : "a member"}.
    </p>
  );

  if (session.status === "signed-in") {
    return (
      <AuthScreen title={title}>
        {subtitle}
        <form onSubmit={accept.submit} className="flex flex-col gap-4">
          <ErrorMessage>{accept.error}</ErrorMessage>
          <Button type="submit" disabled={accept.busy}>
            Join as {session.me.user.name}
          </Button>
          <Button variant="secondary" onClick={() => void signOut()}>
            Not {session.me.user.name}? Sign out
          </Button>
        </form>
      </AuthScreen>
    );
  }

  if (providers.oidc) {
    return (
      <AuthScreen title={title}>
        {subtitle}
        <CodeVillainButton label="Sign in with Code Villain to join" query={{ invite: token }} />
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
