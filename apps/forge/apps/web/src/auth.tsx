import type { InvitePreview, Me } from "@forge/types";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { type FormEvent, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { Redirect } from "./redirect";
import { bundled, DEFAULT_SERVER, normalizeServer, serverUrl, setServerUrl } from "./server";
import { defaultWorkspace, signedIn, signOut, useSession } from "./session";
import { AuthScreen, Button, ErrorMessage, Splash, TextField } from "./ui";

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

/**
 * The packaged desktop app asks which server to use; the browser app is
 * already on it. Returns the field and a function that applies the choice.
 */
function useServerChoice() {
  const [server, setServer] = useState(serverUrl() || DEFAULT_SERVER);
  const field = bundled ? (
    <TextField
      label="Server"
      type="url"
      required
      placeholder={DEFAULT_SERVER}
      hint="Where your team's Forge runs."
      value={server}
      onChange={(e) => setServer(e.target.value)}
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
  return { field, apply };
}

/** Whether this server offers signing in with GitHub (not in the packaged desktop app). */
export function useGitHubLogin() {
  const [available, setAvailable] = useState(false);
  useEffect(() => {
    if (bundled) return;
    api<{ github: boolean }>("GET", "/api/auth/providers")
      .then((p) => setAvailable(p.github))
      .catch(() => {});
  }, []);
  return available;
}

/** What went wrong signing in with GitHub, by the code the server sends back. */
export const GITHUB_PROBLEMS: Record<string, string> = {
  github_not_set_up: "Signing in with GitHub isn't set up on this server.",
  github_expired: "That took too long. Try again.",
  github_other_browser: "Start signing in with GitHub from Forge, in this browser.",
  github_refused: "GitHub didn't sign you in. Try again.",
  github_no_email: "Your GitHub account has no verified email address.",
  github_use_password:
    "There's already an account with your GitHub email. Sign in with its password, then connect GitHub under Account.",
  github_no_account:
    "No Forge account is connected to that GitHub account. Sign up with email, or use an invite link.",
  invite_gone: "That invite link has expired or was revoked. Ask for a new one.",
  github_taken: "That GitHub account is connected to another Forge account.",
  github_failed: "Something went wrong. Try again.",
};

/** Off to GitHub; the server brings people back signed in. */
function GitHubButton({ label, query }: { label: string; query: Record<string, string> }) {
  const params = new URLSearchParams(query).toString();
  return (
    <a
      href={`/api/auth/github${params ? `?${params}` : ""}`}
      className="flex h-9 items-center justify-center gap-2 rounded-md border border-line bg-surface font-medium hover:bg-hover"
    >
      <svg viewBox="0 0 16 16" className="size-4" aria-hidden="true" fill="currentColor">
        <path d="M8 0C3.58 0 0 3.58 0 8a8 8 0 0 0 5.47 7.59c.4.07.55-.17.55-.38v-1.33c-2.23.48-2.7-1.07-2.7-1.07-.36-.92-.89-1.17-.89-1.17-.73-.5.06-.49.06-.49.8.06 1.23.83 1.23.83.72 1.22 1.87.87 2.33.66.07-.52.28-.87.5-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 4 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48v2.2c0 .21.15.46.55.38A8 8 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
      </svg>
      {label}
    </a>
  );
}

function Or() {
  return (
    <div className="my-5 flex items-center gap-3 text-xs text-faint">
      <span className="h-px flex-1 bg-line" />
      or
      <span className="h-px flex-1 bg-line" />
    </div>
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
  const { redirect, error: githubError } = useSearch({ strict: false }) as {
    redirect?: string;
    error?: string;
  };
  const navigate = useNavigate();
  const github = useGitHubLogin();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const server = useServerChoice();
  const { busy, error, submit } = useSubmit(async () => {
    server.apply();
    signedIn(await api<Me>("POST", "/api/auth/login", { email, password }));
    await navigate({ to: safeRedirect(redirect) });
  });

  if (session.status === "signed-in" && !busy) {
    // Connecting GitHub from Account can come back here when its trip can't
    // be read (expired, or the server restarted); say why on Account.
    const workspace = githubError?.startsWith("github") ? defaultWorkspace(session.me) : undefined;
    if (workspace) {
      return (
        <Redirect
          to="/$workspace/settings/account"
          params={{ workspace: workspace.url_key }}
          search={{ github_error: githubError }}
        />
      );
    }
    return <Redirect to={safeRedirect(redirect)} />;
  }
  return (
    <AuthScreen title="Sign in to Forge">
      {github && (
        <>
          <GitHubButton
            label="Continue with GitHub"
            query={redirect ? { redirect: safeRedirect(redirect) } : {}}
          />
          <Or />
        </>
      )}
      {githubError && !error && (
        <div className="mb-4">
          <ErrorMessage>
            {GITHUB_PROBLEMS[githubError] ?? GITHUB_PROBLEMS.github_failed}
          </ErrorMessage>
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
        New to Forge?{" "}
        <Link to="/signup" className="text-fg underline-offset-4 hover:underline">
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
  return (
    <AuthScreen title="Create your Forge account">
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
        <Link to="/login" className="text-fg underline-offset-4 hover:underline">
          Sign in
        </Link>
      </p>
    </AuthScreen>
  );
}

/** Where an invite link lands: join as the signed-in user, or sign up. */
export function JoinPage() {
  const { token } = useParams({ strict: false }) as { token: string };
  const session = useSession();
  const navigate = useNavigate();
  const [invite, setInvite] = useState<InvitePreview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [form, setForm] = useState({ name: "", email: "", password: "" });
  const github = useGitHubLogin();

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
  if (!invite || session.status === "loading") return <Splash />;

  const title = `Join ${invite.workspace_name}`;
  const subtitle = (
    <p className="-mt-3 mb-6 text-center text-muted">
      {invite.invited_by} invited you to their workspace on Forge.
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

  const field = (key: keyof typeof form) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  return (
    <AuthScreen title={title}>
      {subtitle}
      {github && (
        <>
          <GitHubButton label="Join with GitHub" query={{ invite: token }} />
          <Or />
        </>
      )}
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
        <Link
          to="/login"
          search={{ redirect: `/join/${token}` }}
          className="text-fg underline-offset-4 hover:underline"
        >
          Sign in to join
        </Link>
      </p>
    </AuthScreen>
  );
}
