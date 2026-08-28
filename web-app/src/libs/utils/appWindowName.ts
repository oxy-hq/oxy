/**
 * The browser window a custom app is opened into — `target` on every link in
 * the product shell that points at one.
 *
 * ## A custom app never opens inside HQ
 *
 * An app is its own document with its own router, and apps keep navigation
 * state in the query string (Bookkeeping's
 * `?vendor=ubereats&month=2026-07&view=vendors` names a screen). Put it in HQ's
 * browsing context — as an `<iframe>` or as a same-tab navigation — and it stops
 * owning the address bar, which costs three things at once:
 *
 *  - **Back stops being predictable.** A framed app's navigations join the
 *    *parent's* session history, so Back walks the app's history rather than
 *    HQ's and lands a hop or two from where the user came from. A plain same-tab
 *    nav has a milder version: the app rewrites its own URL on mount, pushing
 *    entries HQ never expected.
 *  - **Deep links have nowhere to live.** "UberEats in July" is a URL the app
 *    owns; with HQ's URL in the address bar there is nothing to send anyone.
 *  - **Reload leaves the app.** The address bar says `…/workspaces/<id>/home`,
 *    so a refresh returns to the launcher — worst for the people who use one app
 *    all day.
 *
 * ## Why a name and not `_blank`
 *
 * A name means one tab *per app*: clicking the same card or rail tile again
 * re-targets the tab already open instead of stacking duplicates.
 *
 * That reuse is exactly what `rel="noopener"` / `rel="noreferrer"` disable — a
 * link carrying either is specified to get a fresh browsing context every time —
 * so links using this must set neither. Safe here: these are same-origin bundles
 * we ship, already sharing this origin's cookies and storage, so the opener
 * reference grants nothing the app did not already have.
 *
 * ## Why the org slug is part of the name
 *
 * An app slug is unique **within an org**, but every org shares one origin and
 * therefore one window-name space. Keyed on the app slug alone, two tenants that
 * both call an app `bookkeeping` would share a tab — and the people who hit that
 * are exactly the ones who can least afford it: staff in an assume-role session,
 * a partner across downstream orgs, a consultant in two tenants. Switching org
 * and clicking the tile would navigate the tab they were working in from one
 * tenant's app to another's, possibly in the background, since browsers differ
 * on whether a reused named tab is brought forward.
 */
export function appWindowName(orgSlug: string, slug: string): string {
  return `oxy-app-${orgSlug}-${slug}`;
}
