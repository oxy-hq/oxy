// Release PRs that need no review: see RELEASE_AUTOMATION in config.ts.

import type { ReleaseRule } from "../config.ts";
import type { DiffFile, PrMeta } from "./types.ts";

const VERSION_LINE = /^\s*version\s*=\s*"[^"]*"\s*$/;

/** True only for a version bump the release app opened; content decides, not names. */
export function isReleaseBump(
  { meta, files }: { meta: PrMeta; files: readonly DiffFile[] },
  rule: ReleaseRule
): boolean {
  const author = meta.user;
  if (author.type !== "Bot" || !rule.authors.includes(author.login.replace(/\[bot\]$/, "")))
    return false;
  const ref = meta.head.ref ?? "";
  if (!rule.prefixes.some((p) => ref.startsWith(p))) return false;
  if (!files.length) return false;
  for (const f of files) {
    if (!rule.files.includes(f.path) || f.status !== "modified") return false;
    if (!rule.versionOnly.includes(f.path)) continue;
    for (const h of f.hunks)
      for (const l of h.lines) if (l.type !== " " && !VERSION_LINE.test(l.text)) return false;
  }
  return true;
}
