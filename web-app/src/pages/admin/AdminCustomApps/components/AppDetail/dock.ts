/**
 * Where the dossier sits relative to the live preview.
 *
 * Chrome DevTools' vocabulary, adopted for the same reason DevTools has it:
 * the useful placement depends on what you're reading. Comparing the preview
 * against its status wants a side column; reading wide content (manifest JSON,
 * bundle paths, build rows) wants the stage's full width; working in the app
 * itself while watching its diagnostics wants a second window entirely.
 *
 *   right   — resizable side column (the historical layout, still the default)
 *   bottom  — resizable drawer spanning the whole stage width
 *   window  — undocked into a real second browser window
 */
export type DockMode = "right" | "bottom" | "window";

const DOCK_MODES = ["right", "bottom", "window"] as const;

export const reviveDockMode = (raw: unknown): DockMode | null =>
  DOCK_MODES.includes(raw as DockMode) ? (raw as DockMode) : null;

export const DOCK_STORAGE_KEY = "admin-app-dossier-dock";

/**
 * Route of the standalone dossier page the `window` mode opens.
 *
 * Carries the app's id as `?app=` so the window can read `GET /customer-apps/{id}`
 * directly. It has its own QueryClient — a separate browsing context shares no cache
 * with the opener — and without the id it had to walk the whole paged registry to
 * find one row by slug. The slugs stay in the path for the address bar and for links
 * that predate the param (the window falls back to the walk when `?app=` is absent).
 */
export const dossierWindowPath = (orgSlug: string, appSlug: string, appId?: string) =>
  `/admin/apps/${orgSlug}/${appSlug}/panel${appId ? `?app=${encodeURIComponent(appId)}` : ""}`;

/** The query param `dossierWindowPath` writes; read back by `AppDossierWindow`. */
export const DOSSIER_APP_PARAM = "app";
