/**
 * Identity injected by the backend into `index.html` when the SPA is served
 * on a bare org subdomain (e.g. `pokehouse.oxygen-hq.com`). See
 * `crates/app-core/src/org_host_dispatch.rs` (`window.__OXY_ORG__`).
 *
 * When present the app boots pre-scoped to `orgSlug` + `defaultProjectId`
 * (skipping the org/workspace picker), and the axios 401 handler bounces
 * re-auth to the centralized app host (`appBaseUrl`) instead of a local
 * `/login`.
 */
export interface InjectedOrg {
  orgId: string;
  orgSlug: string;
  subdomain: string;
  /** Admin-set default project; `null` when none chosen yet. */
  defaultProjectId?: string | null;
  /** Centralized auth host, e.g. `https://app.oxygen-hq.com`. */
  appBaseUrl?: string | null;
}

declare global {
  interface Window {
    __OXY_ORG__?: InjectedOrg;
  }
}

/** The injected org context, or `undefined` when not on an org subdomain. */
export function getInjectedOrg(): InjectedOrg | undefined {
  if (typeof window === "undefined") return undefined;
  return window.__OXY_ORG__;
}

/** True when the SPA is running on a bare org subdomain. */
export function isOnOrgSubdomain(): boolean {
  return getInjectedOrg() != null;
}

/**
 * The origin OAuth `redirect_uri`s must use. On an org subdomain this is the
 * centralized app host (`appBaseUrl`) — never `window.location.origin`, which
 * would point a provider at the subdomain (unregistered → `redirect_uri`
 * mismatch). Off a subdomain it's the current origin. Mirrors the backend's
 * `pin_org_subdomain_to_app_host`.
 */
export function authOrigin(): string {
  return getInjectedOrg()?.appBaseUrl ?? window.location.origin;
}

/**
 * Bounce an unauthenticated visitor on an org subdomain to the centralized
 * app-host login (which owns OAuth), carrying a `return_to` back to the
 * current URL. Returns `false` when there's no app host to bounce to (e.g.
 * local dev), so the caller can fall back to local handling.
 */
export function redirectToCentralLogin(): boolean {
  const base = getInjectedOrg()?.appBaseUrl;
  if (!base) return false;
  const returnTo = encodeURIComponent(window.location.href);
  window.location.href = `${base}/login?return_to=${returnTo}`;
  return true;
}
