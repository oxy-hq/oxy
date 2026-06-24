/**
 * Identity injected by the backend into `index.html` when the SPA is served
 * on a bare org subdomain (e.g. `pokehouse.oxygen-hq.com`). See
 * `crates/app/src/server/api/org_host_dispatch.rs` (`window.__OXY_ORG__`).
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
