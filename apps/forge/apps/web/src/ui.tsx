import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { useId } from "react";

type ButtonVariant = "primary" | "secondary" | "danger";

/** A button's look, for links that act as one. */
export function buttonClass(variant: ButtonVariant = "primary"): string {
  const look = {
    primary: "bg-accent-fill text-white hover:brightness-110",
    secondary: "border border-line bg-surface hover:bg-hover",
    danger: "border border-line bg-surface text-danger hover:bg-hover",
  }[variant];
  return `inline-flex h-8 shrink-0 items-center justify-center gap-2 rounded-md px-3 font-medium whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 ${look}`;
}

export function Button({
  variant = "primary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  return <button type="button" {...props} className={`${buttonClass(variant)} ${className}`} />;
}

export function TextField({
  label,
  hint,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: string }) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-xs font-medium text-muted">
        {label}
      </label>
      <input
        id={id}
        {...props}
        className="h-9 rounded-md border border-line-strong bg-surface px-3 outline-none placeholder:text-faint focus:border-accent focus:ring-2 focus:ring-accent/25"
      />
      {hint && <p className="text-xs text-faint">{hint}</p>}
    </div>
  );
}

export function ErrorMessage({ children }: { children: ReactNode }) {
  if (!children) return null;
  return (
    <p role="alert" className="rounded-md bg-danger/10 px-3 py-2 text-danger">
      {children}
    </p>
  );
}

/** The Forge mark. */
export function Logo({ className = "size-5" }: { className?: string }) {
  return <img src="/icon.svg" alt="" className={className} />;
}

/** A centred screen for signing in, signing up and joining. */
export function AuthScreen({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex min-h-full items-center justify-center px-4 py-12">
      <div className="w-full max-w-sm">
        <Logo className="mx-auto mb-6 size-9" />
        <h1 className="mb-6 text-center text-lg font-semibold tracking-tight">{title}</h1>
        {children}
      </div>
    </div>
  );
}

/** Shown while something loads; quiet on purpose. */
export function Splash() {
  return (
    <div className="flex h-full items-center justify-center">
      <Logo className="size-8 animate-pulse opacity-60" />
    </div>
  );
}
