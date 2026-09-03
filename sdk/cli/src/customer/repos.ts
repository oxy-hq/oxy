/**
 * Where OUR repos are on THIS machine — discovered, never assumed.
 *
 * A session working for a customer regularly has to touch one of the shared
 * repos: read how a connector behaves, fix the bug upstream, check what a
 * version actually shipped. Without this, it either guesses a path or clones a
 * second copy of a repo already sitting on disk.
 *
 * DISCOVERY IS BY GIT REMOTE, NOT BY PATH, and that is the whole design. On a
 * real machine every one of these is checked out under a directory named after
 * neither its GitHub org nor, in one case, its repo:
 *
 *   ~/Workspace/github.com/dataframehq/oxy-internal  →  oxy-hq/oxygen-internal
 *   ~/Workspace/github.com/dataframehq/airlayer      →  oxy-hq/airlayer
 *
 * A `<root>/<org>/<name>` convention finds NONE of them and reports four repos
 * as missing while all four are on disk. Reading `origin` finds them whatever
 * the directory is called.
 *
 * IT NEVER CLONES. A repo already on disk is named by its real path; one that
 * is not is named with the command that would fetch it, and the session
 * decides. A tool that pulled gigabytes because a briefing mentioned a repo is
 * one people switch off.
 */

import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { slugFromRemote } from "../util/git.js";
import { ensureDir, oxycCacheDir } from "../util/paths.js";

/**
 * The shared repos, as a CONSTANT of the tool rather than a per-customer
 * field. They are the same for every customer, so a per-customer copy would be
 * the unread-field failure a `repos: []` already was once.
 */
export const SHARED_REPOS = [
  "oxy-hq/oxygen-internal",
  "oxy-hq/airway-internal",
  "oxy-hq/airlayer",
  "oxy-hq/airhouse-internal"
] as const;

/** Directories searched for checkouts. `OXYC_REPO_ROOTS` overrides (colon-separated). */
function repoRoots(): string[] {
  const override = process.env.OXYC_REPO_ROOTS;
  if (override) return override.split(":").filter(Boolean);
  return [
    join(homedir(), "Workspace", "github.com"),
    join(homedir(), "oxy-hq"),
    join(homedir(), "src"),
    join(homedir(), "code"),
    join(homedir(), "Projects")
  ].filter((dir) => existsSync(dir));
}

interface RepoCache {
  fetchedAt: number;
  map: Record<string, string>;
}

function cacheFile(): string {
  // The cache ROOT is closed here as well as the file, because `repos.json`
  // sits directly in it — the per-subdirectory 0700s elsewhere do not cover a
  // file written at the top level. It maps every checkout on this machine,
  // which is a map of what the owner works on.
  return join(ensureDir(oxycCacheDir(), 0o700), "repos.json");
}

/** A day: checkouts move rarely, and a stale entry is validated on read. */
const REPO_CACHE_TTL_MS = 86_400_000;

/**
 * Scan for checkouts, two levels deep under each root.
 *
 * Two levels because the common layouts are `<root>/<org>/<repo>` and
 * `<root>/<repo>`. Deeper would walk into `node_modules` and `target` on a
 * machine that keeps code under its home directory, which costs seconds for
 * repos nobody was looking for.
 */
function scan(): Record<string, string> {
  const map: Record<string, string> = {};
  for (const root of repoRoots()) {
    for (const first of safeDirs(root)) {
      const firstPath = join(root, first);
      record(map, firstPath);
      for (const second of safeDirs(firstPath)) {
        record(map, join(firstPath, second));
      }
    }
  }
  return map;
}

function safeDirs(dir: string): string[] {
  try {
    return readdirSync(dir, { withFileTypes: true })
      .filter((e) => e.isDirectory() && !e.name.startsWith(".") && e.name !== "node_modules")
      .map((e) => e.name);
  } catch {
    return [];
  }
}

function record(map: Record<string, string>, path: string): void {
  if (!existsSync(join(path, ".git"))) return;
  const slug = slugFromRemote(path);
  // First one wins, so a scan that finds two checkouts of the same repo is
  // deterministic rather than depending on directory order.
  if (slug && !map[slug]) map[slug] = path;
}

/** `<org>/<repo>` → local path, for every checkout found. */
export function repoMap(opts: { refresh?: boolean } = {}): Record<string, string> {
  if (!opts.refresh) {
    try {
      const cached = JSON.parse(readFileSync(cacheFile(), "utf8")) as RepoCache;
      if (Date.now() - cached.fetchedAt < REPO_CACHE_TTL_MS) {
        // Validated on read: a checkout that has been moved or deleted since
        // the scan must not be reported as present, because the session would
        // then `cd` into nothing.
        return Object.fromEntries(
          Object.entries(cached.map).filter(([, path]) => existsSync(path))
        );
      }
    } catch {
      // No cache, or an unreadable one. Scan.
    }
  }
  const map = scan();
  try {
    writeFileSync(cacheFile(), JSON.stringify({ fetchedAt: Date.now(), map }), { mode: 0o600 });
  } catch {
    // A cache that cannot be written costs a scan per launch, not an answer.
  }
  return map;
}

/** The local path for one of our repos, if it is on this machine. */
export function localPath(slug: string, opts: { refresh?: boolean } = {}): string | undefined {
  return repoMap(opts)[slug];
}

/**
 * The block a launch briefing embeds: each shared repo, and either where it is
 * or the command that would fetch it.
 */
export function sharedRepoContext(opts: { refresh?: boolean } = {}): string {
  const map = repoMap(opts);
  return SHARED_REPOS.map((slug) => {
    const path = map[slug];
    return path ? `  ${slug} — ${path}` : `  ${slug} — not on this machine (gh repo clone ${slug})`;
  }).join("\n");
}

/**
 * The line a session writes into a cross-repo pull request body.
 *
 * ONE DEFINITION, TWO READERS. The briefing tells a session to write this, and
 * `oxyc activity` matches on exactly it. A briefing that told the session a
 * different spelling would produce pull requests the reader can never find, so
 * neither side is allowed its own copy.
 */
export function attributionLine(customerName: string): string {
  return `Customer: ${customerName}`;
}
