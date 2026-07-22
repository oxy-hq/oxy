/** URL of the workspace logo endpoint (org-uploaded logo, falling back to
 *  the code-first file). Consumers render an `<img>` and fall back on error.
 *  `apiBaseUrl` is the API root (e.g. `"/api"` same-origin, or an absolute
 *  base in dev); pass `version` (the org's `updated_at`) to bust the cache
 *  after an upload/remove. */
export const workspaceLogoUrl = (apiBaseUrl: string, workspaceId: string, version?: string) => {
  const base = `${apiBaseUrl.replace(/\/$/, "")}/${workspaceId}/logo`;
  return version ? `${base}?v=${encodeURIComponent(version)}` : base;
};
