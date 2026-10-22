// The small controls every app's pages are made of, so a field, an error,
// a heading or a name's initial looks the same in each.
import type { LucideIcon } from "lucide-react";
import type { InputHTMLAttributes, ReactNode } from "react";
import { useId } from "react";

export function TextField({
  label,
  hint,
  className = "",
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: string }) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-xs font-medium text-muted">
        {label}
      </label>
      <input id={id} {...props} className={`ui-field h-9 rounded-md px-3 ${className}`} />
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

/** The app's mark: each app serves its own as `/icon.svg`. */
export function Logo({ className = "size-5" }: { className?: string }) {
  return <img src="/icon.svg" alt="" className={className} />;
}

/** A centred screen for signing in, signing up, setting up and joining. */
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

/** A page's empty state: what's missing, and the way to fill it. */
export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="mx-auto flex max-w-md flex-col items-center gap-3 px-4 py-20 text-center">
      <p className="font-medium">{title}</p>
      {children && <div className="text-muted">{children}</div>}
    </div>
  );
}

/** A page's icon, as the sidebar and the command menu show it. */
export function NavIcon({ icon: Icon }: { icon: LucideIcon }) {
  return <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />;
}

/**
 * A neutral initial on the chip tint, in the control role: round for a
 * person, square for an agent or a workspace. Grey does the work; a
 * person's avatar is detail, not a signal.
 */
export function Initial({
  name,
  square = false,
  size = "size-5",
  title,
}: {
  name: string;
  square?: boolean;
  size?: string;
  title?: string;
}) {
  return (
    <span
      className={`${size} ui-control flex shrink-0 items-center justify-center bg-chip text-muted ${square ? "rounded-sm" : "rounded-full"}`}
      aria-hidden="true"
      title={title}
    >
      {name.trim().charAt(0).toUpperCase()}
    </span>
  );
}
