import type { ButtonHTMLAttributes } from "react";

export type ButtonVariant = "primary" | "secondary" | "danger";

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
