// Manifest loader for customer-app bundles served by oxy at
// `app.oxygen-hq.com/customer-apps/<org_slug>/<app_slug>/`.
//
// The bundle commits a `public/oxy-app.json` declaring its identity
// (slug, orgSlug, projectId). This module:
//   1. Fetches that manifest at startup (cached after the first call).
//   2. Validates the schema with clear errors (v2 only — v1 is rejected).
//   3. Joins it with the runtime identity oxy injects via
//      `<script>window.__OXY_APP__=...</script>`.
//
// Bundles call `useQuery` directly for data access — there are no
// `products` or `writers` declarations in v2 manifests.

import { type OxyInjectedAppConfig, readInjectedAppConfig } from "./inject";
import { getOxyAppLogger } from "./logger";

// ── Manifest types ──────────────────────────────────────────────────────────

/** Wire shape of `oxy-app.json` (v2 only). */
export interface OxyAppManifest {
  /** Must be 2. v1 manifests are no longer supported. */
  schemaVersion: 2;
  /**
   * Optional display name. The admin "Link existing" dialog prefills
   * its Name field from this. Omit to let oxy fall back to the
   * folder basename.
   */
  name?: string;
  /**
   * URL slug. **Required.** The canonical source of truth — the
   * dialog locks the slug field to this value, and
   * `OXY_APP_BASE_PATH=/customer-apps/<org>/<slug>/` baked into the
   * build must match.
   */
  slug: string;
  /**
   * Optional org slug. Prefills the dialog's org picker; operator
   * can still override. Carries no security weight — the actual
   * access check is on the linked row.
   */
  orgSlug?: string;
  /**
   * Optional project (workspace) uuid the bundle expects to read
   * from. Used by `useQuery` to construct the
   * `/api/projects/:id/query` URL.
   */
  projectId?: string;
}

// ── Resolved manifest ───────────────────────────────────────────────────────

/**
 * Manifest + runtime-injected identity needed to call oxy. Callers
 * should treat this as the only source of truth for "which org/app
 * does this bundle belong to."
 */
export interface ResolvedCustomerAppManifest {
  manifest: OxyAppManifest;
  /**
   * Always an empty array for v2 manifests. Kept for API compatibility;
   * callers that previously iterated product names should switch to
   * explicit `useQuery` calls.
   * @deprecated Will be removed in a future version.
   */
  productNames: string[];
  /** Org slug injected by oxy. */
  orgSlug: string;
  /** App slug injected by oxy. */
  appSlug: string;
  /**
   * The oxy server's API base URL. Empty string when oxy serves the
   * bundle itself (same-origin, the common case); a full URL only
   * when the bundle is running under a dev server proxy.
   */
  apiBaseUrl: string;
  /** App UUID; informational. */
  appId?: string;
  /**
   * Project (workspace) UUID. Injection (`window.__OXY_APP__.projectId`)
   * wins over the manifest's `projectId` field — the admin row is
   * authoritative. Manifest `projectId` is a dev-time hint used only
   * when running without a server. Used by `useQuery` to construct the
   * `/api/projects/:id/query` URL.
   */
  projectId?: string;
}

export interface LoadManifestOptions {
  /**
   * Override the URL the manifest is fetched from. Default:
   * `<injected_base>/oxy-app.json` or `/oxy-app.json`.
   * Useful for non-Next bundlers — set explicitly to wherever your
   * bundler emits static assets.
   */
  manifestUrl?: string;
}

let cached: Promise<ResolvedCustomerAppManifest> | null = null;

/**
 * Load + validate the manifest. Cached after the first call so callers
 * can invoke this from every component without coordinating.
 */
export function loadCustomerAppManifest(
  options: LoadManifestOptions = {}
): Promise<ResolvedCustomerAppManifest> {
  if (!cached) {
    cached = fetchAndValidate(options);
  }
  return cached;
}

/** For tests: reset the cache between runs. */
export function _resetCustomerAppManifestCacheForTest(): void {
  cached = null;
}

async function fetchAndValidate(
  options: LoadManifestOptions
): Promise<ResolvedCustomerAppManifest> {
  const log = getOxyAppLogger();
  const injected = readInjectedAppConfig();
  const manifestUrl = options.manifestUrl ?? defaultManifestUrl(injected);

  log.log("info", "loading manifest", {
    manifestUrl,
    injectionPresent: !!injected,
    orgSlug: injected?.orgSlug,
    appSlug: injected?.slug,
    appId: injected?.appId
  });

  const startedAt = Date.now();
  const res = await fetch(manifestUrl, { credentials: "same-origin" });
  if (!res.ok) {
    log.log("error", "manifest fetch failed", {
      manifestUrl,
      status: res.status,
      statusText: res.statusText
    });
    throw new Error(
      `Failed to load oxy-app.json from ${manifestUrl} (HTTP ${res.status}). ` +
        `The customer-app repo must commit this file alongside the bundle.`
    );
  }
  const raw = (await res.json()) as unknown;
  const manifest = validateManifest(raw, manifestUrl);

  const resolved: ResolvedCustomerAppManifest = {
    manifest,
    productNames: [],
    orgSlug: injected?.orgSlug ?? "",
    appSlug: injected?.slug ?? "",
    apiBaseUrl: injected?.apiBaseUrl || "",
    appId: injected?.appId,
    projectId: injected?.projectId ?? manifest.projectId
  };
  log.log("info", "manifest ready", {
    durationMs: Date.now() - startedAt,
    schemaVersion: manifest.schemaVersion,
    slug: manifest.slug
  });
  return resolved;
}

/**
 * Default manifest URL.
 *
 * Resolution order (bundler-agnostic):
 *   1. `window.__OXY_APP__.orgSlug`/`slug` injection → the canonical
 *      `/customer-apps/<org>/<app>/oxy-app.json`. Works for every
 *      bundle oxy serves regardless of how it was built.
 *   2. `NEXT_PUBLIC_APP_BASE_PATH` env var — kept for backward compat
 *      with Next.js bundles that bake basePath at build time.
 *   3. Empty basePath → `/oxy-app.json` (only matches when running in
 *      a `vite dev` / `next dev` root mount; will 404 under oxy).
 */
function defaultManifestUrl(injected: OxyInjectedAppConfig | undefined): string {
  if (injected?.orgSlug && injected?.slug) {
    const org = encodeURIComponent(injected.orgSlug);
    const app = encodeURIComponent(injected.slug);
    return `/customer-apps/${org}/${app}/oxy-app.json`;
  }
  // No injection → bundle is running outside oxy (`pnpm dev` against
  // a local Vite, an iframe preview, etc.). Look up `/oxy-app.json`
  // at the document root; the vite-plugin's dev shim and the
  // standard `public/` convention both serve it there.
  return "/oxy-app.json";
}

// ── Validation ──────────────────────────────────────────────────────────────

/**
 * Validate a v2 manifest. Required: schemaVersion === 2, slug (non-empty).
 * Optional: name (display), orgSlug (dev-time hint for the admin dialog),
 * projectId (dev-time hint when there's no server-side injection).
 *
 * At serve time, oxy's identity injection (window.__OXY_APP__) overrides
 * the manifest's orgSlug/projectId — the manifest fields are advisory.
 */
function validateManifest(raw: unknown, url: string): OxyAppManifest {
  if (!isRecord(raw)) {
    throw new Error(`Manifest at ${url} is not a JSON object`);
  }
  if (raw.schemaVersion !== 2) {
    throw new Error(
      `oxy-app.json: schemaVersion must be 2 (got ${JSON.stringify(raw.schemaVersion)}). ` +
        `v1 manifests are no longer supported — upgrade to the identity-only shape.`
    );
  }
  if (raw.products !== undefined || raw.writers !== undefined) {
    throw new Error(
      `oxy-app.json is schemaVersion 2 (identity-only); \`products\` and \`writers\` are no longer supported`
    );
  }
  if (typeof raw.slug !== "string" || !raw.slug.trim()) {
    throw new Error("oxy-app.json: `slug` is required and must be a non-empty string");
  }

  const name = typeof raw.name === "string" ? raw.name : undefined;
  const slug = raw.slug;
  const orgSlug = typeof raw.orgSlug === "string" ? raw.orgSlug : undefined;
  const projectId = typeof raw.projectId === "string" ? raw.projectId : undefined;

  return { schemaVersion: 2, name, slug, orgSlug, projectId };
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}
