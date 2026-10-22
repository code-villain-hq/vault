import { ChevronRight } from "lucide-react";
import { type ButtonHTMLAttributes, type ReactNode, useState } from "react";

export {
  AuthScreen,
  Button,
  buttonClass,
  ErrorMessage,
  Logo,
  Section,
  Splash,
  TextField,
} from "@code-villain/web";

/**
 * The one control that folds a part of a view open and shut: a chevron
 * before its label, turned down when open (components.md, "Disclosure").
 * It doesn't click the row it sits in.
 */
export function Disclosure({
  open,
  onToggle,
  className = "",
  children,
  ...props
}: {
  open: boolean;
  onToggle: () => void;
  className?: string;
  children?: ReactNode;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type" | "onClick" | "className" | "children">) {
  return (
    <button
      {...props}
      type="button"
      aria-expanded={open}
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
      className={`ui-disclosure ${className}`}
    >
      <ChevronRight size={12} strokeWidth={1.75} aria-hidden="true" />
      {children}
    </button>
  );
}

/** A disclosure that keeps its own state, folded at first, over what it opens. */
export function Fold({
  label,
  className,
  buttonClassName,
  children,
}: {
  label: ReactNode;
  className?: string;
  buttonClassName?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className={className}>
      <Disclosure open={open} onToggle={() => setOpen(!open)} className={buttonClassName}>
        {label}
      </Disclosure>
      {open && children}
    </div>
  );
}
