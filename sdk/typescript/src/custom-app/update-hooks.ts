// React hooks over the injected runtime's update agent (`./updates`).
//
// The runtime does the work on its own — detect, announce, and in `auto` mode
// reload at a safe moment — so an app that uses none of these still converges.
// They exist for the two things only the app knows: when a reload would lose
// work (`useBusyWhile`), and how to offer the update itself (`useAppUpdate`).

import * as React from "react";
import { holdUpdates, type OxyUpdateStatus, readUpdateAgent } from "./updates";

const UPDATE_EVENTS = ["oxy:update-status", "oxy:update-available", "oxy:session-expired"];

export interface AppUpdateState {
  /** The live build's id once it differs from this page's; otherwise `null`. */
  available: string | null;
  /** The runtime's last probe was redirected to login: the session ended. */
  sessionExpired: boolean;
  /** The runtime's mode, `undefined` when oxy did not serve this page. */
  mode: OxyUpdateStatus["mode"] | undefined;
  /** Reload onto the new build now. `false` when nothing is pending, or the
   *  loop guard refused (the last two reloads landed on the old build). */
  apply: () => boolean;
  /** Hold automatic reloads; call the returned function to release. */
  defer: (reason: string) => () => void;
}

/**
 * Whether a newer build of this app is live, and the controls to take it.
 *
 * ```tsx
 * const update = useAppUpdate();
 * {update.available && <button onClick={update.apply}>Update ready</button>}
 * ```
 *
 * Reading this does not cancel the runtime's own reload. To take over entirely,
 * listen for `oxy:update-available` and call `preventDefault()` on it.
 */
export function useAppUpdate(): AppUpdateState {
  const [status, setStatus] = React.useState(() => readUpdateAgent()?.status());
  React.useEffect(() => {
    const refresh = () => setStatus(readUpdateAgent()?.status());
    refresh();
    // `oxy:update-status` covers the changes the other two never announce: a
    // rollback to this page's build, a target seen again, a session restored.
    for (const type of UPDATE_EVENTS) window.addEventListener(type, refresh);
    return () => {
      for (const type of UPDATE_EVENTS) window.removeEventListener(type, refresh);
    };
  }, []);
  return {
    available: status?.available ?? null,
    sessionExpired: status?.sessionExpired ?? false,
    mode: status?.mode,
    apply: React.useCallback(() => readUpdateAgent()?.apply() ?? false, []),
    defer: React.useCallback((reason: string) => holdUpdates(reason), [])
  };
}

/**
 * Hold automatic reloads while `busy` is true — a form with unsaved input, a
 * checklist mid-way, an upload in flight. Released when `busy` turns false or
 * the component unmounts.
 *
 * ```tsx
 * useBusyWhile(form.formState.isDirty, "unsaved checklist");
 * ```
 */
export function useBusyWhile(busy: boolean, reason = "busy"): void {
  React.useEffect(() => {
    if (!busy) return;
    return holdUpdates(reason);
  }, [busy, reason]);
}
