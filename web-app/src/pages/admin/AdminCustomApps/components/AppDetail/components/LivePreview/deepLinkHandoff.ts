/**
 * The handoff between "the admin URL says where the preview should be" and
 * "the preview says where it is".
 *
 * Both directions write the same param, so without a rule they fight. The rule
 * has exactly one job: suppress the reports that describe a document the deep
 * link is only *passing through*.
 *
 * ## The transient
 *
 * Opening `?preview=/stores` loads the bundle root first — the frame has to be
 * somewhere before it can be sent anywhere. That root gets reported, the admin
 * URL drops `?preview` as its default value, and a link copied in that window
 * is bare. Self-healing on the next load and written in replace mode, so it
 * never reached the history; visible all the same.
 *
 * ## Why the suppression has to be bounded
 *
 * The first version suppressed anything that was not an exact match on the
 * applied path, with no other exit. A deep-linked app that rewrites its own URL
 * on mount — `/stores` becoming `/stores?tab=all` — therefore never matched,
 * and *every* later report was dropped for the life of the component, across
 * subsequent navigations, because the ref outlived them. The admin URL simply
 * stopped following the preview: this feature, silently off, with nothing to
 * see.
 *
 * So there are two exits, and the second is the one that bounds it: the applied
 * path is reported (it landed where it was aimed), or a new document loads (it
 * landed somewhere else, which is the app's business, not a reason to stop
 * listening). Held as a value rather than a boolean so "in flight to /stores"
 * and "in flight to /reports" are distinguishable.
 */

/** The path a deep link is currently navigating to, or `null` when idle. */
export type DeepLinkHandoff = string | null;

/** A deep link was applied: reports for other paths are the transient. */
export const applying = (target: string): DeepLinkHandoff => target;

/**
 * A new document loaded. Whatever was in flight has landed — wherever it
 * landed — so the handoff is over regardless of where it ended up.
 */
export const landed = (): DeepLinkHandoff => null;

export interface ReportDecision {
  /** Publish this path to the admin URL. */
  publish: boolean;
  /** The handoff state to carry forward. */
  next: DeepLinkHandoff;
}

/**
 * The preview reported where it is. Publish it, unless it is the document a
 * deep link is passing through on the way somewhere else.
 */
export function report(handoff: DeepLinkHandoff, reported: string | null): ReportDecision {
  if (handoff === null) return { publish: true, next: null };
  // Landed on the applied path: the handoff is complete and this is the report
  // that says so.
  if (handoff === reported) return { publish: true, next: null };
  return { publish: false, next: handoff };
}
