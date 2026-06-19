/**
 * Ephemeral, app-wide signal for whether the developer environment — the
 * single-instance backend that owns the working copy, Git, and file edits — is
 * currently reachable. Driven from the Axios interceptor (a non-React source)
 * and surfaced to React via `useSyncExternalStore`, so it is neither server
 * state (React Query) nor a Zustand store; just a tiny pub/sub flag a global
 * banner subscribes to.
 *
 * Detection contract (stamped by the backend, see `crates/app/src/server`):
 *   - unreachable: a `502` carrying `x-oxy-required-role: ide`
 *   - reachable:   any success carrying `x-oxy-served-by: ide`
 *
 * The 502 also carries `x-oxy-unavailable: workspace-{runtime,editing}` (which
 * capability is down) and `Retry-After`. The GLOBAL banner intentionally does
 * not branch on the class — when the ide is unreachable BOTH classes fail
 * together — but it's available for per-surface inline handling (e.g. a chart
 * widget) and for ops alerting per class.
 */

import { isAxiosError } from "axios";

/**
 * True when `error` is the ide-down `502` — it carries `x-oxy-required-role:
 * ide` (the same signal the Axios interceptor uses to raise the global banner).
 * Surfaces that need the developer environment (data apps, charts, runs) use
 * this to render a calm inline "restarting" placeholder instead of a generic
 * error, so an ide restart reads as "paused, resuming" rather than "broken".
 */
export function isIdeUnavailableError(error: unknown): boolean {
  if (!isAxiosError(error)) return false;
  const res = error.response;
  if (res?.status !== 502) return false;
  const headers = res.headers as Record<string, string | undefined> | undefined;
  return headers?.["x-oxy-required-role"] === "ide";
}

export interface IdeHealthState {
  /** The developer environment is currently unreachable. */
  unavailable: boolean;
  /** Last request path that failed, shown in the banner's Details line. */
  lastPath: string | null;
  /** Epoch ms when the current outage was first observed. */
  since: number | null;
  /** User closed the banner for the current outage. */
  dismissed: boolean;
}

const INITIAL: IdeHealthState = {
  unavailable: false,
  lastPath: null,
  since: null,
  dismissed: false
};

let state: IdeHealthState = INITIAL;
const listeners = new Set<() => void>();

function setState(next: IdeHealthState): void {
  state = next;
  for (const listener of listeners) listener();
}

/** Drop a leading workspace-UUID segment so the Details line stays short and
 *  doesn't surface an internal identifier. */
function tidyPath(path: string): string {
  return path.replace(/^\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\//i, "/");
}

export function reportIdeUnavailable(path: string): void {
  const tidied = tidyPath(path);
  if (state.unavailable) {
    // Already in an outage — refresh the failing path but don't re-open a
    // banner the user has dismissed for this outage.
    if (state.lastPath !== tidied) setState({ ...state, lastPath: tidied });
    return;
  }
  setState({ unavailable: true, lastPath: tidied, since: Date.now(), dismissed: false });
}

export function reportIdeReachable(): void {
  if (state === INITIAL) return;
  // Recovery clears the dismissal too, so a later outage surfaces afresh.
  setState(INITIAL);
}

export function dismissIdeUnavailableBanner(): void {
  if (!state.dismissed) setState({ ...state, dismissed: true });
}

export function subscribeIdeHealth(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getIdeHealthSnapshot(): IdeHealthState {
  return state;
}
