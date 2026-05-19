// Repo-relative file-path safety helper for `browser_file_upload`.
//
// Policy (mirrors fixtures/reset.ts → restoreDemoFile):
//   - Path must be a non-empty string, repo-relative (no leading `/`).
//   - No `..` segments; no absolute paths.
//   - No symbolic-link traversal; resolved real path must stay under the
//     repo root.
//   - The file must exist.
//
// Rationale: a flow author (or LLM) that emits `~/.aws/credentials` or an
// absolute path through `/etc/` should fail loudly rather than silently
// uploading whatever happens to live at that path. The same parent-walk
// safety check used for restoring committed demo files applies here.

import { existsSync, lstatSync, realpathSync } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..");
const REAL_REPO_ROOT = realpathSync(REPO_ROOT);

export function resolveRepoFile(rel: string): string {
  if (typeof rel !== "string" || rel.length === 0) {
    throw new Error("browser_file_upload: empty path");
  }
  if (isAbsolute(rel)) {
    throw new Error(
      `browser_file_upload: absolute paths refused (got ${JSON.stringify(
        rel
      )}). Use a path relative to the repo root.`
    );
  }
  if (rel.split(/[/\\]/).some((seg) => seg === "..")) {
    throw new Error(`browser_file_upload: path may not contain '..' (got ${JSON.stringify(rel)})`);
  }
  const target = resolve(REPO_ROOT, rel);
  let probe = target;
  while (probe !== dirname(probe)) {
    try {
      const stat = lstatSync(probe);
      if (stat.isSymbolicLink()) {
        throw new Error(`browser_file_upload refuses to operate through a symbolic link: ${probe}`);
      }
      const real = realpathSync(probe);
      if (!real.startsWith(REAL_REPO_ROOT)) {
        throw new Error(
          `browser_file_upload refuses to upload from outside the repo (resolved ${probe} → ${real})`
        );
      }
      break;
    } catch (err) {
      const code = (err as NodeJS.ErrnoException).code;
      if (code === "ENOENT") {
        probe = dirname(probe);
        continue;
      }
      throw err;
    }
  }
  if (!existsSync(target)) {
    throw new Error(`browser_file_upload: file not found at ${target}`);
  }
  return target;
}
