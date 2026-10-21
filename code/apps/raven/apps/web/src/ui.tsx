import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { useId } from "react";

type ButtonVariant = "primary" | "secondary" | "danger";

/** A button's look, for links that act as one: the shared ui-button recipe. */
export function buttonClass(variant: ButtonVariant = "primary"): string {
  return {
    primary: "ui-button ui-button-primary",
    secondary: "ui-button",
    danger: "ui-button ui-button-danger",
  }[variant];
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
      <input id={id} {...props} className="ui-field h-9 rounded-md px-3" />
      {hint && <p className="text-xs text-faint">{hint}</p>}
    </div>
  );
}

export function ErrorMessage({ children }: { children: ReactNode }) {
  if (!children) return null;
  return (
    <p role="alert" className="ui-tint-danger rounded-md px-3 py-2">
      {children}
    </p>
  );
}

/** The Raven mark. */
export function Logo({ className = "size-5" }: { className?: string }) {
  return <img src="/icon.svg" alt="" className={className} />;
}

/** A centred screen for signing in and setting up. */
export function AuthScreen({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex min-h-full items-center justify-center px-4 py-12">
      <div className="w-full max-w-sm">
        {/* Clear space: a quarter of the mark's 80px height. */}
        <Logo className="mx-auto mb-5 size-20" />
        <h1 className="mb-6 text-center text-title font-semibold">{title}</h1>
        {children}
      </div>
    </div>
  );
}

/** Shown while something loads; quiet on purpose. */
export function Splash() {
  return (
    <div className="flex h-full items-center justify-center">
      <Logo className="size-8 animate-breathe" />
    </div>
  );
}

/** A labelled part of a page: a quiet heading over its content. */
export function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-baseline gap-3">
        <h2 className="text-xs font-semibold text-muted">{title}</h2>
        {aside && <div className="ml-auto text-xs text-faint">{aside}</div>}
      </div>
      {children}
    </section>
  );
}

/** One number with its label: the headline figures of a page. */
export function Stat({
  label,
  value,
  note,
  tone,
}: {
  label: string;
  value: ReactNode;
  note?: ReactNode;
  /** Status ink, only when the number needs a look. */
  tone?: "warn" | "danger";
}) {
  const ink = tone === "danger" ? "text-danger-ink" : tone === "warn" ? "text-warn-ink" : "text-fg";
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg bg-surface px-4 py-3">
      <span className="text-xs text-muted">{label}</span>
      <span className={`text-title font-semibold tabular-nums ${ink}`}>{value}</span>
      {note && <span className="truncate text-xs text-faint">{note}</span>}
    </div>
  );
}

/** The window a page looks at, as the shared segmented control. */
export function WindowPicker({
  value,
  onChange,
}: {
  value: number;
  onChange: (secs: number) => void;
}) {
  const windows: [number, string][] = [
    [900, "15m"],
    [3600, "1h"],
    [86_400, "24h"],
    [604_800, "7d"],
  ];
  return (
    <fieldset className="ui-segmented" aria-label="Time window">
      {windows.map(([secs, label]) => (
        <button
          key={secs}
          type="button"
          className="ui-segment"
          aria-pressed={value === secs}
          onClick={() => onChange(secs)}
        >
          {label}
        </button>
      ))}
    </fieldset>
  );
}
