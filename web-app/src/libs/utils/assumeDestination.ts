/**
 * Where an assume-role session should take you, and where stopping it should
 * bring you back to.
 *
 * Starting and ending a session are both **hard navigations** (see
 * `useStartAssume` / `useEndAssume`) — the identity behind every request changes,
 * so the app is re-initialised from scratch. Nothing in React memory survives
 * that, which is why the round trip is parked in `localStorage` rather than in
 * state or a store.
 *
 * The default round trip is org-shaped: you land on the tenant's product and come
 * back to `/admin/tenants`. That is right when you assumed a role *from* the org
 * list, and wrong when you assumed it from one specific app — you wanted **that
 * app**, and you want to come back to the page you left. This record carries that
 * intent across the two reloads.
 *
 * Two deliberate constraints:
 *
 * - **Paths only, never URLs.** Both values are fed to `window.location.assign`,
 *   and this record is user-writable (it is localStorage). Accepting only
 *   same-origin absolute paths means a tampered entry can at worst send an
 *   operator to another page of Oxy — never off-site.
 * - **It expires with the session.** `MAX_SESSION` is 60 minutes and
 *   non-renewable, so a record older than that describes a session that no longer
 *   exists. Past the ceiling it is ignored and the caller falls back to its own
 *   default.
 */

/** Mirrors `assume::MAX_SESSION` (60 minutes, non-renewable). */
const SESSION_MINUTES = 60;
const KEY = "oxy.assume.destination";

interface AssumeDestination {
  /** The org being assumed. Scoping by org keeps one app's round trip from
   *  hijacking an unrelated assume started somewhere else. */
  orgId: string;
  /** Where to go once the session starts. Consumed on arrival. */
  landing: string | null;
  /** Where to return when the session ends. Lives as long as the session. */
  returnTo: string | null;
  /** Epoch ms. */
  expiresAt: number;
}

const SAFE_PATH_SENTINEL = "https://oxy-same-origin.invalid";

/**
 * A same-origin absolute path — the only thing we will ever navigate to. Both
 * legs of the round-trip come from user-writable localStorage and are fed
 * straight to `window.location.assign`, so this is the whole guarantee that a
 * tampered entry can at worst move an operator to another page of Oxy, never
 * off-site.
 *
 * We resolve the value through the browser's OWN URL parser — the same one
 * `assign` uses — and require the result to stay on a fixed sentinel origin.
 * That catches every normalisation bypass, not just the ones an allowlist
 * happens to name: protocol-relative `//evil.com`, backslash forms, AND the
 * ASCII tab/newline/CR that the URL spec strips before parsing (so `"/\t/evil.com"`
 * would otherwise collapse to `//evil.com` → off-origin). The cheap `startsWith`
 * guards keep it a path (not a same-origin absolute URL) and short-circuit the
 * obvious cases.
 */
function isSafePath(value: unknown): value is string {
  if (typeof value !== "string" || !value.startsWith("/") || value.startsWith("//")) {
    return false;
  }
  try {
    return new URL(value, SAFE_PATH_SENTINEL).origin === SAFE_PATH_SENTINEL;
  } catch {
    return false;
  }
}

function read(): AssumeDestination | null {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<AssumeDestination>;
    if (typeof parsed.orgId !== "string" || typeof parsed.expiresAt !== "number") return null;
    // Purely read-only: `peekAssumeReturnTo` calls this during render, so we must
    // not write here. An expired record is simply ignored; the single key is
    // overwritten on the next session start and cleared on session end, so at
    // most one dead record ever lingers — no need to prune it mid-render.
    if (parsed.expiresAt <= Date.now()) return null;
    return {
      orgId: parsed.orgId,
      landing: isSafePath(parsed.landing) ? parsed.landing : null,
      returnTo: isSafePath(parsed.returnTo) ? parsed.returnTo : null,
      expiresAt: parsed.expiresAt
    };
  } catch {
    // Unparseable or storage-denied (private mode, disabled cookies). The round
    // trip degrades to the org-shaped default — never to a broken page.
    return null;
  }
}

/**
 * Record the round trip for the session about to start. Call it immediately
 * before starting, so a session that never starts leaves nothing behind past the
 * ceiling.
 */
export function rememberAssumeDestination(input: {
  orgId: string;
  landing: string;
  returnTo: string;
}): void {
  if (!isSafePath(input.landing) || !isSafePath(input.returnTo)) return;
  try {
    const record: AssumeDestination = {
      orgId: input.orgId,
      landing: input.landing,
      returnTo: input.returnTo,
      expiresAt: Date.now() + SESSION_MINUTES * 60_000
    };
    window.localStorage.setItem(KEY, JSON.stringify(record));
  } catch {
    // Storage unavailable — the caller's default landing still applies.
  }
}

/**
 * The landing for a session that just started, consumed on read: it describes one
 * arrival, and leaving it set would redirect a later assume of the same org to a
 * page nobody asked for. `returnTo` is deliberately kept — the trip home has not
 * happened yet.
 */
export function takeAssumeLanding(orgId: string): string | null {
  const record = read();
  if (!record || record.orgId !== orgId || !record.landing) return null;
  try {
    window.localStorage.setItem(KEY, JSON.stringify({ ...record, landing: null }));
  } catch {
    // Best effort: a landing we can't clear is still a landing we can honour.
  }
  return record.landing;
}

/**
 * Where "stop acting" should return this operator. Read-only — safe to call
 * during render. Returns `null` when there is no recorded trip home, so the
 * caller applies its own default.
 */
export function peekAssumeReturnTo(orgId: string | undefined): string | null {
  if (!orgId) return null;
  const record = read();
  return record && record.orgId === orgId ? record.returnTo : null;
}

/** Forget the round trip — the session ended, or was never started. */
export function clearAssumeDestination(): void {
  try {
    window.localStorage.removeItem(KEY);
  } catch {
    // Nothing to do; an expired record is ignored on read anyway.
  }
}
