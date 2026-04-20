export type CtaState = "conflict" | "commit" | "push" | "pull" | "pr" | "fetch" | "none";

export interface CtaInputs {
  isOnMain: boolean;
  isAhead: boolean;
  isConflict: boolean;
  hasLocalChanges: boolean;
  showPull: boolean;
  showOpenPr: boolean;
  canPush: boolean;
}

/**
 * Decide which primary call-to-action button to show in the IDE header.
 *
 * Rules are evaluated top-to-bottom; the first match wins. Order encodes
 * priority — a conflict always pre-empts a pending commit, an unpushed
 * commit pre-empts a pull, etc. Returning `"none"` means no primary CTA
 * (the branch pill / history button still render separately).
 *
 * Pure function so it's trivially unit-testable.
 */
export function deriveCtaState(i: CtaInputs): CtaState {
  if (i.isConflict) return "conflict";
  if (i.hasLocalChanges) return "commit";
  if (!i.isOnMain && i.isAhead) return "push";
  if (i.showPull) return "pull";
  if (i.showOpenPr) return "pr";
  if (!i.isOnMain && i.canPush) return "fetch";
  return "none";
}
