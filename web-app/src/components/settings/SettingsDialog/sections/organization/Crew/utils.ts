import { isAxiosError } from "axios";
import type { AppAccessSummary } from "@/types/appAccess";
import type { FrontlineWorker } from "@/types/frontline";

/** The server's PIN policy, mirrored so a bad PIN never leaves the form. */
export const PIN_PATTERN = /^\d{4,8}$/;

/** Why a PIN pair can't be submitted, or null when it can. */
export function pinProblem(pin: string, confirm: string): string | null {
  if (!PIN_PATTERN.test(pin)) return "A PIN is 4 to 8 digits.";
  if (pin !== confirm) return "The two PINs don't match.";
  return null;
}

export type WorkerStanding = "active" | "suspended" | "locked";

/**
 * `locked` is a transient state on top of an active worker — too many wrong
 * PINs — and a PIN reset clears it. A suspended worker reads as suspended
 * whether or not a lockout is also running, because that is the one an admin
 * chose.
 */
export function workerStanding(worker: FrontlineWorker, now = Date.now()): WorkerStanding {
  if (worker.status === "suspended") return "suspended";
  if (worker.locked_until && new Date(worker.locked_until).getTime() > now) return "locked";
  return "active";
}

const APP_PATH_PREFIX = "/customer-apps";

/**
 * Where a kiosk lands after sign-in. The server checks it against the
 * deployment's allowlist, and the org's own custom apps are the only
 * destinations this dialog offers — so the URL is composed, never typed.
 */
export function appReturnTo(orgSlug: string, appSlug: string): string {
  return `${window.location.origin}${APP_PATH_PREFIX}/${orgSlug}/${appSlug}/`;
}

/**
 * The app a stored `return_to` points at, if it is one of ours. Matched on
 * path rather than the whole URL so a kiosk enrolled from another host of the
 * same deployment still resolves to its app name.
 */
export function appForReturnTo(
  apps: AppAccessSummary[],
  orgSlug: string,
  returnTo: string | null
): AppAccessSummary | undefined {
  if (!returnTo) return undefined;
  let path: string;
  try {
    path = new URL(returnTo).pathname.replace(/\/+$/, "");
  } catch {
    // Not a URL at all — nothing to resolve; the caller shows the raw value.
    return undefined;
  }
  return apps.find((app) => path === `${APP_PATH_PREFIX}/${orgSlug}/${app.slug}`);
}

/**
 * The message an org route put in its error body. The crew routes answer
 * `{ error }`; older org routes answer `{ message }`. Neither is guaranteed.
 */
export function apiErrorMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) {
    const data: unknown = err.response?.data;
    if (data && typeof data === "object") {
      const body = data as { error?: unknown; message?: unknown };
      if (typeof body.error === "string" && body.error) return body.error;
      if (typeof body.message === "string" && body.message) return body.message;
    }
    return fallback;
  }
  return err instanceof Error && err.message ? err.message : fallback;
}

export function apiStatus(err: unknown): number | undefined {
  return isAxiosError(err) ? err.response?.status : undefined;
}

/** Order-insensitive equality for two id lists — the "anything to save?" check. */
export function sameIds(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const set = new Set(a);
  return b.every((id) => set.has(id));
}
