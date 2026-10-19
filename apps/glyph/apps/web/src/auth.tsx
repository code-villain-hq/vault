import type { InvitePreview, Me } from "@glyph/types";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { type FormEvent, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { Redirect } from "./redirect";
import { bundled, DEFAULT_SERVER, normalizeServer, serverUrl, setServerUrl } from "./server";
import { signedIn, signOut, useSession } from "./session";
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
      hint="Where your team's Glyph runs."
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

/** Only same-site paths, so a link can't send people elsewhere after signing in. */
function safeRedirect(path: string | undefined) {
  // Not "//host" or "/\host" (browsers read both as another site), nor
  // control characters.
  if (!path?.startsWith("/") || /^\/[/\\]/.test(path)) return "/";
  return [...path].some((c) => c.charCodeAt(0) < 32) ? "/" : path;
}

export function LoginPage() {
  const session = useSession();
  const { redirect } = useSearch({ strict: false }) as { redirect?: string };
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const server = useServerChoice();
  const { busy, error, submit } = useSubmit(async () => {
    server.apply();
    signedIn(await api<Me>("POST", "/api/auth/login", { email, password }));
    await navigate({ to: safeRedirect(redirect) });
  });

  if (session.status === "signed-in" && !busy) {
    return <Redirect to={safeRedirect(redirect)} />;
  }
  return (
    <AuthScreen title="Sign in to Glyph">
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

/** Where an invite link lands: join as the signed-in user, or sign up. */
export function JoinPage() {
  const { token } = useParams({ strict: false }) as { token: string };
  const session = useSession();
  const navigate = useNavigate();
  const [invite, setInvite] = useState<InvitePreview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [form, setForm] = useState({ name: "", email: "", password: "" });

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
      {invite.invited_by} invited you to their workspace on Glyph.
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
