/**
 * Where this tool keeps things on disk.
 *
 * Cache and state are separated on purpose: everything under `cacheDir()` is
 * reconstructible from the network and may be deleted at any moment by the OS
 * or by `oxyc cache clear`, while `stateDir()` holds things whose loss costs
 * something. Putting a token in a cache directory is how a Mac's periodic
 * cleanup logs you out.
 */

import { chmodSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** `dirs::cache_dir()`, matching the Rust crate's platform rules. */
export function cacheDir(): string {
  if (process.platform === "darwin") return join(homedir(), "Library", "Caches");
  if (process.platform === "win32") {
    return process.env.LOCALAPPDATA ?? join(homedir(), "AppData", "Local");
  }
  const xdg = process.env.XDG_CACHE_HOME;
  if (xdg?.startsWith("/")) return xdg;
  return join(homedir(), ".cache");
}

/** This tool's cache root. `OXYC_CACHE_DIR` overrides, for tests. */
export function oxycCacheDir(): string {
  return process.env.OXYC_CACHE_DIR ?? join(cacheDir(), "oxyc");
}

/**
 * `mkdir -p`, returning the path so it composes into a `join`.
 *
 * `mode` is worth passing for anything holding response bodies or tokens:
 * the default is 0755, so a directory of cached authenticated responses would
 * be readable by every other user on the machine.
 */
export function ensureDir(path: string, mode?: number): string {
  mkdirSync(path, mode === undefined ? { recursive: true } : { recursive: true, mode });
  // `mkdir`'s mode applies only to a directory it CREATES. A cache directory
  // written by an earlier version — before these were tightened — already
  // exists at 0755, so without this every existing install would keep serving
  // world-readable response bodies and nothing would say so. Best-effort: a
  // filesystem with no POSIX modes must not fail the command over it.
  if (mode !== undefined) {
    try {
      chmodSync(path, mode);
    } catch {
      // A Windows share, or a directory owned by someone else. The caller's
      // work is not worth failing for a permission bit it cannot set.
    }
  }
  return path;
}

/**
 * A filesystem-safe fragment of an arbitrary string.
 *
 * Two different inputs can collapse onto one name, which is why every cache
 * file that uses this ALSO stores its true key inside the file and checks it
 * on read: the filename is the convenience, the stored field is the identity.
 */
export function slugifyForPath(value: string): string {
  return value.replace(/[^A-Za-z0-9._-]/g, "_");
}
