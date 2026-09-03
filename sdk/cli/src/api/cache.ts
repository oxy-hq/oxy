/**
 * `--cache <duration>` — reuse a recent successful response instead of asking
 * again.
 *
 * `gh api` has this and it matters more here than there: the caller is often
 * an agent walking the same few endpoints repeatedly inside one debugging
 * session, and the second `oxyc api {org}/workspaces` in ninety seconds is
 * asking a question whose answer cannot have changed.
 *
 * ONLY SUCCESSFUL, SAFE REQUESTS ARE CACHED. A cached 500 would turn a
 * transient outage into a sticky one, and a cached POST would silently drop
 * the second half of "create it twice" — so the write path refuses anything
 * that is not a 2xx GET/HEAD, rather than trusting callers to pass `--cache`
 * only where it is safe.
 */

import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { usageError } from "../util/errors.js";
import { ensureDir, oxycCacheDir } from "../util/paths.js";

interface CacheEntry {
  /** The full request key, stored so a filename collision can be detected. */
  key: string;
  status: number;
  headers: Record<string, string>;
  body: string;
  storedAt: number;
}

/**
 * Parse a Go-style duration (`30s`, `5m`, `2h`) into milliseconds.
 *
 * gh's spelling, so the flag transfers. A bare number is refused rather than
 * assumed to be seconds: `--cache 60` meaning a minute in one tool and an
 * hour in another is exactly the ambiguity a unit suffix removes.
 */
export function parseDuration(value: string): number {
  const match = /^(\d+)(ms|s|m|h)$/.exec(value.trim());
  if (!match) {
    throw usageError(`invalid --cache '${value}', expected a duration like 30s, 5m or 2h`);
  }
  const amount = Number(match[1]);
  switch (match[2]) {
    case "ms":
      return amount;
    case "s":
      return amount * 1000;
    case "m":
      return amount * 60_000;
    default:
      return amount * 3_600_000;
  }
}

/**
 * The cache key: everything that can change the answer.
 *
 * The token is included via its hash — two people sharing a laptop must not
 * read each other's responses, and the same endpoint genuinely answers
 * differently per caller (`/api/user`, anything org-scoped). Hashed rather
 * than stored so the key itself is not a second place a bearer token lives.
 */
export function cacheKey(
  method: string,
  url: string,
  body: string | undefined,
  token: string | undefined
): string {
  const identity = token ? createHash("sha256").update(token).digest("hex").slice(0, 16) : "anon";
  return `${method} ${url} ${identity} ${body ? createHash("sha256").update(body).digest("hex") : ""}`;
}

function entryPath(key: string): string {
  const digest = createHash("sha256").update(key).digest("hex");
  // 0700, and the entries below are written 0600. These files hold the BODIES
  // of authenticated multi-tenant responses — a customer's rows, an org's
  // member list — and the cache key already hashes the token precisely so two
  // users on one machine cannot read each other's. Leaving the bytes
  // world-readable would hand back exactly what that key is protecting.
  // Credentials take 0600 for the same reason; so should these.
  return join(ensureDir(join(oxycCacheDir(), "responses"), 0o700), `${digest}.json`);
}

/** A cached response, if there is a fresh one under this exact key. */
export function readCache(key: string, maxAgeMs: number): CacheEntry | undefined {
  try {
    const entry = JSON.parse(readFileSync(entryPath(key), "utf8")) as CacheEntry;
    // The filename is a hash and hashes can in principle collide; the stored
    // key is the identity. Cheap to check, and a wrong-body response served
    // as right would be indistinguishable from a server bug.
    if (entry.key !== key) return undefined;
    if (Date.now() - entry.storedAt > maxAgeMs) return undefined;
    return entry;
  } catch {
    return undefined;
  }
}

/** Store a response, if it is one that may be stored. */
export function writeCache(
  key: string,
  method: string,
  status: number,
  headers: Record<string, string>,
  body: string
): void {
  const safe = method === "GET" || method === "HEAD";
  if (!safe || status < 200 || status >= 300) return;
  const entry: CacheEntry = { key, status, headers, body, storedAt: Date.now() };
  try {
    writeFileSync(entryPath(key), JSON.stringify(entry), { mode: 0o600 });
  } catch {
    // A cache that cannot be written is a slow tool, not a broken one.
  }
}

/** Everything this CLI writes under the cache root. */
const KNOWN_CACHES = ["responses", "catalog", "customers", "repos.json"] as const;

/**
 * Drop EVERY cache this CLI writes, and report what went.
 *
 * `oxyc cache clear` used to call `clearResponseCache` alone while advertising
 * "responses, route catalogs and customer listings" — so the catalog, the
 * customer list and the repo scan all survived a command whose whole purpose
 * is to make the next call ask again. Someone clearing the cache to escape a
 * stale answer got the stale answer back.
 *
 * Clears what `KNOWN_CACHES` lists, BY NAME. A cache added later is therefore
 * not covered automatically — it survives, and shows up in
 * `unknownCacheEntries`, which is the deliberate trade: sweeping the whole
 * root would cover it, and would also `rm -rf` whatever else an
 * `OXYC_CACHE_DIR` pointed at a shared directory happened to contain.
 */
export function clearAllCaches(): string[] {
  const root = oxycCacheDir();
  // Nothing cached yet is the ordinary state on a fresh machine, and `cache
  // clear` is precisely the command somebody runs when they are not sure. It
  // must not be the one that throws ENOENT at them.
  if (!existsSync(root)) return [];
  const cleared: string[] = [];
  for (const entry of KNOWN_CACHES) {
    const path = join(root, entry);
    if (existsSync(path)) {
      rmSync(path, { recursive: true, force: true });
      cleared.push(entry);
    }
  }
  return cleared;
}

/**
 * Entries under the cache root this function does not know how to clear.
 *
 * NAMED, NOT DELETED. `clearAllCaches` used to sweep the whole root so that a
 * cache added later was covered without anyone remembering — but
 * `OXYC_CACHE_DIR` is a plain env override, so pointed at a shared directory,
 * `$XDG_CACHE_HOME`, or a mistyped path, that sweep is an unannounced `rm -rf`
 * of somebody else's files. Reporting the strays keeps the property that
 * mattered (an omission shows up in the output) with none of the risk.
 */
export function unknownCacheEntries(): string[] {
  const root = oxycCacheDir();
  if (!existsSync(root)) return [];
  try {
    return readdirSync(root, { withFileTypes: true })
      .map((e) => e.name)
      .filter((name) => !(KNOWN_CACHES as readonly string[]).includes(name));
  } catch {
    return [];
  }
}
