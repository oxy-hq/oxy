// Bundle-side accessor for the server's diagnostic snapshot.
//
// `GET /api/customer-apps/<org>/<app>/debug` returns a structured
// snapshot of what oxy currently sees about a registered customer
// app: the app row, bundle dir resolution, and parsed manifest (or
// parse error). Useful when a bundle isn't loading what you expected
// and you want to verify what the server actually sees — without
// needing terminal access.
//
// The `products` field on the snapshot is a legacy artifact carried
// for server-side compatibility; in v2 the bundle owns its queries
// via `useQuery` and the field is always empty for v2 manifests.

import { getOxyAppLogger } from "./logger";
import type { ResolvedCustomerAppManifest } from "./manifest";

/** Untyped at the boundary — keep it loose so server-side schema
 * additions don't break older bundles. Stable enough for inspection
 * but not a contract clients should depend on field-by-field. */
export interface CustomerAppDebugSnapshot {
  org_slug: string;
  app_slug: string;
  app: {
    id: string;
    slug: string;
    name: string;
    status: string;
    source_type: string;
    project_id: string;
    branch: string;
  };
  bundle_dir: string | null;
  bundle_dir_exists: boolean;
  /** Raw parsed manifest from the server — kept loose so schema additions don't break older bundles. */
  manifest: Record<string, unknown> | null;
  manifest_error: string | null;
  products: Array<{ name: string; producer: string }>;
}

/**
 * Fetch the server-side diagnostic snapshot for this bundle. Pair with
 * `loadCustomerAppManifest()` — pass its result here. Logs the
 * snapshot through the SDK logger so it appears in the bundle's
 * console at info level.
 */
export async function getCustomerAppDebug(
  resolved: ResolvedCustomerAppManifest
): Promise<CustomerAppDebugSnapshot> {
  const log = getOxyAppLogger();
  const { apiBaseUrl, orgSlug, appSlug } = resolved;
  const url =
    `${apiBaseUrl}/api/customer-apps/` +
    `${encodeURIComponent(orgSlug)}/${encodeURIComponent(appSlug)}/debug`;

  log.log("debug", "fetching debug snapshot", { url });
  const res = await fetch(url, { credentials: "same-origin" });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(
      `Failed to fetch debug snapshot (HTTP ${res.status}): ${detail || res.statusText}`
    );
  }
  const snapshot = (await res.json()) as CustomerAppDebugSnapshot;
  log.log("info", "debug snapshot", snapshot as unknown as Record<string, unknown>);
  return snapshot;
}
