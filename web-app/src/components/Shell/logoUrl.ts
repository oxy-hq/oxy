import { apiBaseURL } from "@/services/env";

/** Same-origin (prod) / dev-server (dev) URL of the workspace logo endpoint
 *  (org-uploaded logo, falling back to the code-first file). Consumers
 *  render an <img> and fall back on error. Pass `version` (the org's
 *  `updated_at`) to bust the cache after an upload/remove. */
export const workspaceLogoUrl = (workspaceId: string, version?: string) => {
  const base = `${apiBaseURL}/${workspaceId}/logo`;
  return version ? `${base}?v=${encodeURIComponent(version)}` : base;
};
