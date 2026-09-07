/**
 * The trailing-window choices offered wherever a period preset is picked —
 * the World Model scan controls and the Metric Tree's scenario toolbar share
 * this one list so neither surface can drift to offer a preset the other
 * would treat as unrecognized.
 *
 * Zero-dependency by design: this module must never import from the API
 * client layer (or anything that transitively does), so the pure scenario
 * modules (`scenarioUrl.ts` and its tests) can pull in `PRESET_DAYS` without
 * dragging in `window.location` reads at module-eval time.
 */
export const PRESET_DAYS = [30, 90, 180, 365] as const;
