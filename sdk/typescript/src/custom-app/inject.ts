// Runtime app-config injected into the browser by oxy when it serves
// a custom-app bundle's HTML. Lets a single bundle serve any
// registered app without having `(orgId, projectId)` baked in at
// build time — see
// `crates/app/src/server/api/custom_apps_serve.rs::inject_app_config`
// on the server side.

/**
 * Shape of `window.__OXY_APP__` written by oxy at serve time.
 * Consumed by `loadCustomAppManifest` as the authoritative identity
 * source (overrides any hints in `oxy-app.json`).
 */
export interface OxyInjectedAppConfig {
  appId: string;
  slug: string;
  orgId: string;
  orgSlug: string;
  projectId: string;
  branch: string;
  /** Empty string means same-origin (the default for v2). */
  apiBaseUrl: string;
}

declare global {
  interface Window {
    __OXY_APP__?: OxyInjectedAppConfig;
  }
}

/**
 * Read the runtime app-config oxy injected at serve time. Returns
 * `undefined` outside the browser or when the global isn't set
 * (`pnpm dev` against a non-oxy server, etc. — manifest hints are
 * the fallback).
 */
export function readInjectedAppConfig(): OxyInjectedAppConfig | undefined {
  if (typeof window === "undefined") return undefined;
  return window.__OXY_APP__;
}
