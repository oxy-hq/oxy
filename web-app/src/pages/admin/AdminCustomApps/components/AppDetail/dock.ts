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

/** Route of the standalone dossier page the `window` mode opens. */
export const dossierWindowPath = (orgSlug: string, appSlug: string) =>
  `/admin/apps/${orgSlug}/${appSlug}/panel`;
