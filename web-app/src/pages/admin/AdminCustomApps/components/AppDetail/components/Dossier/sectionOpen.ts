import type { SectionId } from "../../appViewState";

/**
 * Is this section expanded?
 *
 * Three inputs and they are not symmetric. `stored` is the operator's own
 * preference and is authoritative when it says open. `focusSection` is the URL
 * asking for one section — an override laid ON TOP of the stored map rather
 * than into it, so a colleague's link answers its question without re-filing
 * this operator's layout.
 *
 * `dismissedFor` is what keeps that override from being a one-way door. Without
 * it, closing the focused section writes `false` into the stored map, the
 * override re-opens it on the same render, and the collapsible reads as broken
 * with no escape short of editing the URL. Holding *which* focus was dismissed,
 * rather than a boolean, is what makes a second link open its own section
 * without an effect to reset a flag.
 */
export function isSectionOpen(
  stored: boolean,
  id: SectionId,
  focusSection: SectionId | null | undefined,
  dismissedFor: SectionId | null
): boolean {
  if (stored) return true;
  return id === focusSection && dismissedFor !== focusSection;
}
