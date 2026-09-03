/**
 * Unpacking the assets that a compiled `oxyc` carries inside itself.
 *
 * `bun build --compile` produces one executable and nothing else, so the
 * `template/`, `skills/` and `json-schemas/` directories cannot be read off
 * disk the way they are from an npm install. `scripts/embed-assets.mjs` inlines
 * them into `../generated/embedded-assets.ts`; this unpacks that on demand.
 *
 * **Why extract to disk rather than serve from memory.** `oxyc skills install`
 * symlinks into the skills directory and `oxyc new` copies a directory tree —
 * both need real paths that other processes (Claude Code, git) can open. A
 * virtual filesystem would have meant rewriting every consumer to go through an
 * asset reader and would still not have given `skills install` something to
 * point a symlink at.
 *
 * The extraction directory is keyed by the payload digest, so a new release
 * unpacks alongside the old one instead of reusing it. It lives under the cache
 * root, which is documented as disposable — that is correct here: losing it
 * costs one re-extraction, because the bytes are in the executable.
 */

import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
  writeFileSync
} from "node:fs";
import { dirname, join } from "node:path";
import { gunzipSync } from "node:zlib";
import { ASSET_DIRS, ASSETS_DIGEST, ASSETS_GZIP_BASE64 } from "../generated/embedded-assets.js";
import { CliError, ExitCode } from "../util/errors.js";
import { ensureDir, oxycCacheDir } from "../util/paths.js";

export { ASSET_DIRS };

/**
 * What to tell someone whose installation is missing its assets.
 *
 * Every branch that can report a broken install shares this, so the curl URL
 * exists once — four hand-copied URLs is how one of them ends up stale.
 */
export const REINSTALL_REMEDY =
  "reinstall: `curl -fsSL https://raw.githubusercontent.com/oxy-hq/oxygen/main/install_oxyc.sh | bash`";

/** One embedded file: base64 contents, plus the executable bit when it had one. */
interface EmbeddedFile {
  d: string;
  x?: 1;
}

/** Where a given build's assets unpack to. Digest-keyed, so upgrades never collide. */
export function embeddedAssetsDir(): string {
  return join(oxycCacheDir(), "assets", ASSETS_DIGEST);
}

/** True once every embedded directory is present at `root`. */
function looksComplete(root: string): boolean {
  return ASSET_DIRS.every((dir) => existsSync(join(root, dir)));
}

/**
 * Materialise the embedded assets and return the directory holding them.
 *
 * Writes into a sibling temporary directory and renames it into place, so a
 * second `oxyc` running concurrently — or one killed halfway — can never leave
 * a half-written tree that later runs would treat as complete.
 */
export function extractEmbeddedAssets(): string {
  const target = embeddedAssetsDir();
  if (looksComplete(target)) return target;

  const parent = ensureDir(dirname(target));
  const staging = mkdtempSync(join(parent, `.${ASSETS_DIGEST}.tmp-`));
  try {
    let files: Record<string, EmbeddedFile>;
    try {
      files = JSON.parse(gunzipSync(Buffer.from(ASSETS_GZIP_BASE64, "base64")).toString("utf8"));
    } catch (cause) {
      throw new CliError("the assets embedded in this build are corrupt", {
        code: ExitCode.FAILURE,
        detail: cause instanceof Error ? cause.message : String(cause),
        remedy: REINSTALL_REMEDY
      });
    }

    for (const [relative, file] of Object.entries(files)) {
      const dest = join(staging, ...relative.split("/"));
      mkdirSync(dirname(dest), { recursive: true });
      writeFileSync(dest, Buffer.from(file.d, "base64"));
      // `template/scripts/dev.sh` and the CI scripts are executable in the
      // repo. A scaffolded workspace whose `dev.sh` is 0644 fails in a way that
      // reads as a bug in the customer's repo rather than in this extraction.
      if (file.x) chmodSync(dest, 0o755);
    }

    if (!looksComplete(staging)) {
      throw new CliError("the assets embedded in this build are incomplete", {
        code: ExitCode.FAILURE,
        remedy: REINSTALL_REMEDY
      });
    }

    try {
      renameSync(staging, target);
    } catch {
      // `rename` onto a non-empty directory fails, and there are two ways one
      // can be sitting there.
      //
      // Another process won the race and put its own COMPLETE tree there
      // first. Theirs is byte-identical — same digest — so use it, drop ours.
      if (looksComplete(target)) {
        rmSync(staging, { recursive: true, force: true });
        return target;
      }
      // Or a previous run was killed part-way and left a PARTIAL tree. Without
      // clearing it, every future run fails the same rename and the install is
      // wedged until someone finds the cache directory by hand — the failure
      // this whole staging dance exists to prevent.
      rmSync(target, { recursive: true, force: true });
      try {
        renameSync(staging, target);
      } catch (retryErr) {
        // Lost a race to a process that completed between the remove and the
        // retry. Its tree is the same bytes; anything else is a real failure.
        if (!looksComplete(target)) throw retryErr;
        rmSync(staging, { recursive: true, force: true });
      }
    }
    return target;
  } catch (err) {
    rmSync(staging, { recursive: true, force: true });
    throw err;
  }
}
