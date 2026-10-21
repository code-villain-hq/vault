import {
  type AnyRouter,
  type NavigateOptions,
  type RegisteredRouter,
  useNavigate,
} from "@tanstack/react-router";
import { useLayoutEffect, useRef } from "react";

/**
 * Replaces the current page with another, once. TanStack's `<Navigate>`
 * navigates again whenever its props change identity, which loops when the
 * page re-renders mid-navigation (for example while sync catches up).
 * Typed like `<Navigate>`, so routes and params are checked.
 */
export function Redirect<
  TRouter extends AnyRouter = RegisteredRouter,
  const TFrom extends string = string,
  const TTo extends string | undefined = undefined,
  const TMaskFrom extends string = TFrom,
  const TMaskTo extends string = "",
>(props: NavigateOptions<TRouter, TFrom, TTo, TMaskFrom, TMaskTo>): null {
  const navigate = useNavigate();
  const done = useRef(false);
  const latest = useRef(props);
  latest.current = props;
  useLayoutEffect(() => {
    if (done.current) return;
    done.current = true;
    void navigate({ ...latest.current, replace: true } as never);
  }, [navigate]);
  return null;
}
