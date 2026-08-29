/**
 * The preview's own history stack, kept out of the admin console's.
 *
 * ## Why this exists
 *
 * A same-origin `<iframe>` shares the **joint session history** with the page
 * that contains it. Every navigation inside the previewed app — its router
 * calling `pushState`, a link click, a hash change — appends an entry to the
 * admin console's own Back stack. So an operator who clicks around inside a
 * dashboard and then presses Back does not return to the app list; they step
 * backwards inside the iframe, invisibly, while the admin UI stays put. Press
 * it enough times and they land somewhere neither surface named.
 *
 * This is the same defect the homepage app dock had, and there it was fixed by
 * refusing to nest at all. Admin cannot take that exit: previewing an app
 * beside its manifest, builds and request log **is** the surface.
 *
 * So the preview keeps its own stack instead. Everything below is bookkeeping
 * for one idea: the framed app may navigate as much as it likes, and none of it
 * reaches the joint history.
 *
 * ## The rules a navigation has to obey
 *
 * Three ways a framed document adds a joint entry, and each needs its own
 * answer (`installGuard` in `usePreviewHistory.ts` applies all three):
 *
 *  1. `history.pushState` — the SPA router case, and the common one. Patched to
 *     delegate to `replaceState`, which mutates the current entry instead of
 *     adding one.
 *  2. A same-document **hash** change. Also answered by `replaceState`, because
 *     assigning `location.hash` would add an entry and `location.replace` with
 *     only a fragment difference would pointlessly reload the document.
 *  3. A real navigation — an `<a href>` the app does not intercept, or a form
 *     submit. Answered by converting the click to `location.replace`, which
 *     navigates without growing the stack.
 *
 * What is deliberately not intercepted: modified clicks (cmd/ctrl/shift/alt,
 * middle button), `target=`, `download`, and anything cross-origin. Those mean
 * "not here" — they open a tab, which is the one navigation that cannot
 * pollute this history because it does not happen in this browsing context.
 *
 * ## The trust argument
 *
 * Reaching into another window's `history` is invasive, and it is sound here
 * for the reason the same file already gives for shipping no `sandbox`
 * attribute: this frame loads a bundle we ship, from our own origin. The
 * platform's own injected runtime patches the same two methods, for pageview
 * instrumentation. If the frame is ever not same-origin, every accessor below
 * throws and the caller degrades to "no preview history" rather than breaking
 * the preview.
 */

/** One visited location inside the preview, as an absolute URL string. */
export type PreviewEntry = string;

export interface PreviewHistoryState {
  /** Visited locations, oldest first. Empty until the frame first loads. */
  entries: PreviewEntry[];
  /** Index into `entries` of the location currently displayed. */
  index: number;
}

export const EMPTY_HISTORY: PreviewHistoryState = { entries: [], index: -1 };

export const canGoBack = (s: PreviewHistoryState) => s.index > 0;
export const canGoForward = (s: PreviewHistoryState) =>
  s.index >= 0 && s.index < s.entries.length - 1;
export const currentEntry = (s: PreviewHistoryState): PreviewEntry | null =>
  s.index >= 0 ? (s.entries[s.index] ?? null) : null;

/**
 * Record a location the preview has moved to.
 *
 * Truncates anything ahead of the cursor, the way a browser does: navigating
 * after going Back discards the forward entries, because they are no longer
 * reachable from where you are.
 *
 * A repeat of the current location is a no-op rather than a new entry — an app
 * that `replaceState`s the same URL on every render (normalising a query
 * string, say) would otherwise fill the stack with a location the operator
 * cannot tell apart from the one before it, and Back would appear broken for
 * the opposite reason.
 */
export function pushEntry(s: PreviewHistoryState, url: PreviewEntry): PreviewHistoryState {
  if (currentEntry(s) === url) return s;
  const entries = [...s.entries.slice(0, s.index + 1), url];
  return { entries, index: entries.length - 1 };
}

/** Rewrite the current location in place — the `replaceState` shape. */
export function replaceEntry(s: PreviewHistoryState, url: PreviewEntry): PreviewHistoryState {
  if (s.index < 0) return { entries: [url], index: 0 };
  if (s.entries[s.index] === url) return s;
  const entries = [...s.entries];
  entries[s.index] = url;
  return { entries, index: s.index };
}

/** Move the cursor. Out-of-range deltas clamp rather than throw — a double
 *  click on Back at the start of the stack should do nothing, not crash. */
export function moveCursor(s: PreviewHistoryState, delta: number): PreviewHistoryState {
  if (s.index < 0) return s;
  const next = Math.min(Math.max(s.index + delta, 0), s.entries.length - 1);
  return next === s.index ? s : { ...s, index: next };
}

/**
 * True when a click on `anchor` would navigate this browsing context — the only
 * case worth converting to a `replace`. Everything else either opens elsewhere
 * (so it cannot touch this history) or is not a navigation at all.
 */
export function shouldInterceptAnchor(
  anchor: { href: string; target: string; hasDownload: boolean },
  origin: string
): boolean {
  if (anchor.hasDownload) return false;
  // A target names another browsing context — including `_blank`, which is the
  // gesture that deliberately leaves this one.
  if (anchor.target && anchor.target !== "_self") return false;
  if (!anchor.href) return false;
  try {
    return new URL(anchor.href, origin).origin === origin;
  } catch {
    // `mailto:`, `tel:`, a malformed href — not a same-document navigation.
    return false;
  }
}

/** Two URLs that differ only by fragment: a same-document navigation, which
 *  `replaceState` can absorb without reloading anything. */
export function isSameDocument(a: string, b: string): boolean {
  try {
    const ua = new URL(a);
    const ub = new URL(b);
    return ua.origin === ub.origin && ua.pathname === ub.pathname && ua.search === ub.search;
  } catch {
    return false;
  }
}
