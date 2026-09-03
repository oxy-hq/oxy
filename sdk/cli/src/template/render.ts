/**
 * Rendering the template, and the provenance stamp that records what a repo
 * was rendered FROM.
 *
 * A SYNC IS A RE-RENDER, NOT A COPY, and both halves of that had to be learned
 * by running against a real customer repo:
 *
 *   1. Template files carry `__NAME__`, `__SLUG__` and `__WORKSPACE__`, so a
 *      plain copy writes the literal placeholder into a live repo.
 *   2. The COMPARISON must substitute too. Diffing raw template bytes reports
 *      every placeholder-bearing file as drifted on every run — a report
 *      people stop reading, which costs more than not having one.
 */

import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";

import { CliError, ExitCode } from "../util/errors.js";
import { currentBranch, headSha, isRepo, workingTreeState } from "../util/git.js";

/** The file recording what a repo was rendered from. */
export const STAMP_FILE = ".oxyc-template.json";

/** The provenance value that means "oxyc did NOT render this tree". */
export const PROVENANCE_IMPORTED = "imported";

export interface Substitutions {
  slug: string;
  name: string;
  /** The workspace path relative to the repo root. `.` for a root workspace. */
  workspace: string;
}

/**
 * Substitute placeholders in file CONTENTS — never in paths.
 *
 * A file with no placeholder keeps its EXACT bytes, which is why the
 * comparison is `substituted === original` rather than an unconditional
 * rewrite: an empty `.gitkeep` must stay empty rather than becoming a stray
 * newline, and a byte-identical file must stay byte-identical so the drift
 * report does not invent a difference.
 */
export function substitute(content: string, subs: Substitutions): string {
  const out = content
    .replaceAll("__SLUG__", subs.slug)
    .replaceAll("__NAME__", subs.name)
    .replaceAll("__WORKSPACE__", subs.workspace);
  return out;
}

/**
 * The two directions of one rename: the template ships `_gitignore`, a repo
 * gets `.gitignore`.
 *
 * The underscore is not a style choice. npm strips a file literally named
 * `.gitignore` out of a published tarball, so a template shipping one works
 * perfectly from a checkout and arrives on npm with no ignore rules at all —
 * which commits `node_modules/` and `out/` on the customer's first
 * `git add -A`. `sdk/create-oxy-app`'s templates use the same convention for
 * the same reason.
 *
 * Both directions live here, next to each other, because they must stay exact
 * inverses: a walk that renames one way and a lookup that renames the other
 * are how a file gets classified under a path it is never installed at.
 */
export function repoPathFor(templateRel: string): string {
  return templateRel.replace(/(^|\/)_gitignore$/, "$1.gitignore");
}

/** The inverse of [`repoPathFor`]. */
export function templateSourceFor(repoRel: string): string {
  return repoRel.replace(/(^|\/)\.gitignore$/, "$1_gitignore");
}

/** Every file under `root`, as paths relative to it. Directories are skipped. */
export function walkFiles(root: string, prefix = ""): string[] {
  const out: string[] = [];
  let entries: import("node:fs").Dirent[];
  try {
    entries = readdirSync(join(root, prefix), { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    // `.git` is the one directory a template tree must never carry into a
    // render — it would make the rendered repo a clone of the tooling.
    if (entry.name === ".git") continue;
    if (entry.isDirectory()) out.push(...walkFiles(root, rel));
    else out.push(rel);
  }
  return out.sort();
}

/**
 * Render `src` into `dest`, substituting as it goes.
 *
 * REFUSES AN EXISTING DESTINATION OUTRIGHT, so a partial render over somebody's
 * work is not a state this can reach.
 */
export function renderTemplate(src: string, dest: string, subs: Substitutions): string[] {
  if (existsSync(dest) && readdirSync(dest).length > 0) {
    throw new CliError(`${dest} already exists and is not empty`, {
      code: ExitCode.REFUSED,
      hint: "rendering into it could overwrite work — move it aside, or pick another destination"
    });
  }

  const written: string[] = [];
  for (const rel of walkFiles(src)) {
    const from = join(src, rel);
    const target = repoPathFor(rel);
    const to = join(dest, target);
    mkdirSync(dirname(to), { recursive: true });
    writeFileSync(to, substitute(readFileSync(from, "utf8"), subs), {
      mode: statSync(from).mode & 0o777
    });
    written.push(target);
  }
  return written;
}

/** The provenance document. `syncable` is DERIVED, never passed in. */
export interface Stamp {
  _comment: string;
  stamp_version: 1;
  generated_by: string;
  rendered_at: string | null;
  provenance: string;
  /** A tree oxyc rendered may be synced; an imported one may not. */
  syncable: boolean;
  source_commit: string | null;
  source_branch: string | null;
  source_dirty: boolean | null;
}

const STAMP_COMMENT =
  "Provenance for this workspace: what oxyc rendered it from. Written by `oxyc new` " +
  "from the customer-tooling template; read later by `oxyc update` to report how far " +
  "this repo has drifted from that template. Machine-written — edit the repo, not this file.";

/**
 * Build the stamp document.
 *
 * `syncable` is derived here so there is exactly ONE rule and no caller can
 * write a document whose two halves disagree. Every unanswerable question is
 * written down as `null` rather than guessed at.
 */
export function buildStamp(input: {
  sha?: string;
  branch?: string;
  dirty?: boolean;
  provenance: string;
  renderedAt?: string;
  by: string;
}): Stamp {
  return {
    _comment: STAMP_COMMENT,
    stamp_version: 1,
    generated_by: input.by,
    rendered_at: input.renderedAt ?? null,
    provenance: input.provenance,
    syncable: input.provenance !== PROVENANCE_IMPORTED,
    source_commit: input.sha ?? null,
    source_branch: input.branch ?? null,
    source_dirty: input.dirty ?? null
  };
}

/**
 * Write the stamp into `dest`, recording the state of the template tree `src`.
 *
 * THREE STATES, and only three, so a reader never has to interpret:
 *
 *   clean    `source_commit` describes the bytes that were copied
 *   dirty    the commit is known but the template tree had uncommitted
 *            changes, so the rendered bytes are NOT that commit's
 *   unknown  no commit could be determined at all
 *
 * `dirty` exists because the alternative is a lie: a stamp naming a commit
 * whose contents were not what got rendered makes a later drift report
 * confidently wrong, which is worse than no report. A drift check must refuse
 * to compute against anything but `clean`.
 *
 * NEVER FAILS THE SCAFFOLD. A customer's repo matters more than its
 * provenance, so the file is written either way.
 */
export function writeStamp(dest: string, src: string, by: string, renderedAt: string): Stamp {
  let sha: string | undefined;
  let branch: string | undefined;
  let dirty: boolean | undefined;

  // `git` may be absent, and `src` may be an ordinary directory that was never
  // a repository — a fixture, an unpacked tarball, a clone whose `.git` was
  // stripped. Both are ordinary here, not errors.
  try {
    if (isRepo(src)) {
      sha = headSha(src);
      if (sha) {
        branch = currentBranch(src);
        const state = workingTreeState(src);
        dirty = state.dirty || state.untracked.length > 0;
      }
    }
  } catch {
    // Leaves everything `unknown`, which is the honest answer.
  }

  const stamp = buildStamp({
    sha,
    branch,
    dirty,
    provenance: sha ? (dirty ? "dirty" : "clean") : "unknown",
    renderedAt,
    by
  });
  try {
    writeFileSync(join(dest, STAMP_FILE), `${JSON.stringify(stamp, null, 2)}\n`);
  } catch {
    // Provenance is a nicety; the repo is the product.
  }
  return stamp;
}

/** Read a repo's stamp, or `undefined` when it has none or it is unreadable. */
export function readStamp(repoDir: string): Stamp | undefined {
  try {
    return JSON.parse(readFileSync(join(repoDir, STAMP_FILE), "utf8")) as Stamp;
  } catch {
    return undefined;
  }
}

/**
 * May `oxyc update --apply` rewrite files in this repo?
 *
 * The proof is the stamp: no stamp, or one saying `provenance: "imported"`,
 * means NO. Dropping our workflows beside somebody else's four is the failure
 * the manifest exists to prevent, and `oxyc adopt` is the one explicit act
 * that converts "refused forever" into "syncable".
 */
export function isSyncable(repoDir: string): boolean {
  const stamp = readStamp(repoDir);
  if (!stamp) return false;
  if (stamp.provenance === PROVENANCE_IMPORTED) return false;
  return stamp.syncable !== false;
}

/** Which command wrote the stamp — the one fact that tells `new` and `adopt` apart. */
export function generatedBy(repoDir: string): string | undefined {
  return readStamp(repoDir)?.generated_by;
}

/** Relative path helper shared by the sync and adopt walks. */
export function relativeTo(root: string, path: string): string {
  return relative(root, path).split("\\").join("/");
}
