import { useNavigate, useSearch } from "@tanstack/react-router";
import { type FormEvent, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { Redirect } from "./redirect";
import { signedIn, useSession } from "./session";
import type { Me } from "./types";
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

/** The ways in this server offers, and whether it still needs its first account. */
export type Providers = { oidc: boolean; password: boolean; setup: boolean };

/** Before the server answers, or when it can't: email and password. */
const PASSWORD_ONLY: Providers = { oidc: false, password: true, setup: false };

/** How Raven signs people in; null until it first says, so the page doesn't flash. */
export function useProviders() {
  const [providers, setProviders] = useState<Providers | null>(null);
  useEffect(() => {
    let current = true;
    fetch("/api/auth/providers")
      .then((r) => (r.ok ? (r.json() as Promise<Providers>) : PASSWORD_ONLY))
      .catch(() => PASSWORD_ONLY)
      .then((p) => {
        if (current) setProviders(p);
      });
    return () => {
      current = false;
    };
  }, []);
  return providers;
}

/** What went wrong signing in with Code Villain, by the code the server sends back. */
const OIDC_PROBLEMS: Record<string, string> = {
  oidc_not_set_up: "Signing in with Code Villain isn't set up on this server.",
  oidc_expired: "That took too long. Try again.",
  oidc_other_browser: "Start signing in from Raven, in this browser.",
  oidc_refused: "Code Villain didn't sign you in. Try again.",
  oidc_no_email: "Your Code Villain account has no email address Raven can use.",
  oidc_taken: "The Raven account with your email is linked to another Code Villain account.",
  oidc_failed: "Something went wrong. Try again.",
};

/** A Code Villain sign-in problem we have words for; unknown codes are the general one. */
function oidcCode(code: string) {
  return Object.hasOwn(OIDC_PROBLEMS, code) ? code : "oidc_failed";
}

/** Off to the studio's sign-in; the server brings people back signed in. */
function CodeVillainButton({ label, query }: { label: string; query: Record<string, string> }) {
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
  const providers = useProviders();

  if (session.status === "signed-in") return <Redirect to={safeRedirect(redirect)} />;
  if (!providers || session.status === "loading") return <Splash />;
  const problem = problemCode ? (OIDC_PROBLEMS[oidcCode(problemCode)] ?? null) : null;
  const query: Record<string, string> = redirect ? { redirect: safeRedirect(redirect) } : {};

  if (providers.oidc) {
    return (
      <AuthScreen title="Sign in to Raven">
        {problem && (
          <div className="mb-4">
            <ErrorMessage>{problem}</ErrorMessage>
          </div>
        )}
        <CodeVillainButton label="Sign in with Code Villain" query={query} />
        <p className="mt-6 text-center text-muted">
          One account for every Code Villain app.
          <br />
          New here? Ask your team for an invite.
        </p>
      </AuthScreen>
    );
  }
  if (providers.setup) return <SetupForm redirect={redirect} />;
  return <PasswordForm redirect={redirect} problem={problem} />;
}

function PasswordForm({ redirect, problem }: { redirect?: string; problem: string | null }) {
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const { busy, error, submit } = useSubmit(async () => {
    signedIn(await api<Me>("POST", "/api/auth/login", { email, password }));
    await navigate({ to: safeRedirect(redirect) });
  });
  return (
    <AuthScreen title="Sign in to Raven">
      {problem && !error && (
        <div className="mb-4">
          <ErrorMessage>{problem}</ErrorMessage>
        </div>
      )}
      <form onSubmit={submit} className="flex flex-col gap-4">
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
    </AuthScreen>
  );
}

/** A Raven without accounts asks for its first one; after that, nobody signs up. */
function SetupForm({ redirect }: { redirect?: string }) {
  const navigate = useNavigate();
  const [form, setForm] = useState({ name: "", email: "", password: "" });
  const field = (key: keyof typeof form) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  const { busy, error, submit } = useSubmit(async () => {
    signedIn(await api<Me>("POST", "/api/auth/signup", form));
    await navigate({ to: safeRedirect(redirect) });
  });
  return (
    <AuthScreen title="Set up Raven">
      <p className="-mt-3 mb-6 text-center text-muted">
        Raven has no accounts yet. Yours is the first, and the only one with a password.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <TextField label="Your name" autoComplete="name" required {...field("name")} />
        <TextField label="Email" type="email" autoComplete="email" required {...field("email")} />
        <TextField
          label="Password"
          type="password"
          autoComplete="new-password"
          required
          minLength={10}
          hint="At least 10 characters."
          {...field("password")}
        />
        <ErrorMessage>{error}</ErrorMessage>
        <Button type="submit" disabled={busy}>
          {busy ? "Setting up…" : "Set up Raven"}
        </Button>
      </form>
    </AuthScreen>
  );
}
