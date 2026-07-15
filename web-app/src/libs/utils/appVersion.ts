/** Build identifier baked into this bundle at build time (see vite.config.ts).
 * Compared against the deployed /version.json to detect that a newer build has
 * shipped while this tab stayed open. */
export const APP_VERSION = __APP_VERSION__;

const VERSION_URL = "/version.json";
const FETCH_TIMEOUT_MS = 5000;

/**
 * Read the `version.json` emitted alongside the deployed bundle. Returns `null`
 * when the endpoint is unreachable, mid-deploy, times out, or was answered by
 * the SPA fallback with non-JSON — callers decide what a `null` means in their
 * context.
 */
export const fetchDeployedVersion = async (): Promise<string | null> => {
  try {
    const res = await fetch(VERSION_URL, {
      cache: "no-store",
      signal: AbortSignal.timeout(FETCH_TIMEOUT_MS)
    });
    if (!res.ok) return null;
    const data: unknown = await res.json();
    if (
      data !== null &&
      typeof data === "object" &&
      "version" in data &&
      typeof data.version === "string"
    ) {
      return data.version;
    }
  } catch {
    // Offline, mid-deploy, timed out, or SPA-fallback HTML.
  }
  return null;
};

/**
 * True when the deployed build differs from the one running in this tab.
 * A `null` deployed version (couldn't read it) is treated as "no newer build
 * confirmed" so callers that poll don't false-alarm on a transient blip.
 */
export const isNewVersionDeployed = async (): Promise<boolean> => {
  const deployed = await fetchDeployedVersion();
  return deployed !== null && deployed !== APP_VERSION;
};
