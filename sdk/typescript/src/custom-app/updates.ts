// The SDK half of "a long-open tab picks up a new build".
//
// Oxy injects a small runtime into every served custom app. Its update agent
// polls `<base>__oxy/build`, announces a new build with a cancelable
// `oxy:update-available` event, and — when the platform or the app's manifest
// says `auto` — reloads at a safe moment. It exposes `window.__oxyUpdate`,
// which this module wraps so app code never touches the global directly.
//
// Everything here degrades to a no-op when the runtime is absent (local
// `pnpm dev`, tests, a page oxy did not serve).

import { readInjectedAppConfig } from "./inject";

/** What the injected runtime reports about updates. */
export interface OxyUpdateStatus {
  /** `off`, `observe`, `auto` or `prompt` — see `performance.autoUpdate`. */
  mode: "off" | "observe" | "auto" | "prompt";
  /** The build this document runs (`window.__OXY_APP__.buildId`). */
  current: string;
  /** The live build, once it differs from `current`; otherwise `null`. */
  available: string | null;
  /** The last probe was redirected to login. */
  sessionExpired: boolean;
}

/** `window.__oxyUpdate`, installed by the runtime oxy injects. */
export interface OxyUpdateAgent {
  /** Hold off automatic reloads until the returned function is called. A
   *  token older than 12 h is ignored, so a leaked one cannot pin a build. */
  defer(reason?: string): () => void;
  /** Reload onto the new build now, if one is known. */
  apply(): boolean;
  /** Ask the server which build is live, now. Resolves to its id, or `null`
   *  when the answer was unknown. */
  check(): Promise<string | null>;
  status(): OxyUpdateStatus;
}

declare global {
  interface Window {
    __oxyUpdate?: OxyUpdateAgent;
  }
}

/** The runtime's update agent, or `undefined` where oxy did not serve the page. */
export function readUpdateAgent(): OxyUpdateAgent | undefined {
  if (typeof window === "undefined") return undefined;
  return window.__oxyUpdate;
}

/** Hold off automatic reloads while `reason` is true of the page; the returned
 *  function releases the hold. A no-op outside an oxy-served page. */
export function holdUpdates(reason: string): () => void {
  return readUpdateAgent()?.defer(reason) ?? (() => {});
}

/** Request header: the build this document was served. */
export const CLIENT_BUILD_HEADER = "x-oxy-client-build";
/** Response header on every `/fn` answer: the build the function ran in. */
export const LIVE_BUILD_HEADER = "x-oxy-live-build";

type Fetcher = typeof fetch;

/**
 * Tell oxy which build a function call comes from, and react when the answer
 * says that build is no longer live.
 *
 * `/fn` always runs the LIVE build. A page loaded before a publish learns of it
 * here before its own poll does: a different `x-oxy-live-build`, or a
 * `409 ClientBuildStale` for a function the live build no longer has, makes the
 * runtime check at once (it still applies its own floor and safe moments).
 *
 * Skipped when `backendUrl` is set, for the same reason as `withAppHeader`: a
 * custom header on a cross-origin call forces a CORS preflight.
 */
export function withClientBuild(
  base: Fetcher,
  backendUrl?: string,
  buildId: string | undefined = readInjectedAppConfig()?.buildId
): Fetcher {
  if (backendUrl || !buildId) return base;
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    if (typeof input !== "string" || !/\/customer-apps\/[^/]+\/[^/]+\/fn\//.test(input)) {
      return base(input, init);
    }
    const headers = new Headers(init?.headers);
    if (!headers.has(CLIENT_BUILD_HEADER)) headers.set(CLIENT_BUILD_HEADER, buildId);
    const resp = await base(input, { ...init, headers });
    const live = resp.headers?.get?.(LIVE_BUILD_HEADER);
    if (resp.status === 409 || (live && live.toLowerCase() !== buildId.toLowerCase())) {
      void readUpdateAgent()?.check();
    }
    return resp;
  }) as Fetcher;
}
