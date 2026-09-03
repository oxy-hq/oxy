/**
 * The drift engine behind `oxyc update <customer>`: what a customer's repo
 * would look like if the template were rendered into it again, which of those
 * differences oxyc is allowed to do anything about, and — with `--apply` — the
 * writing of the ones it owns.
 *
 * READING IS THE DEFAULT AND WRITING IS THE FLAG, because the safe direction
 * has to be the one you get by accident.
 *
 * THE WALK IS OVER THE TEMPLATE, NEVER OVER THE CUSTOMER'S REPO. A file the
 * template does not ship is never visited, so it can never be named, counted
 * or written. That is what makes `unmatched` safe by construction rather than
 * by remembering.
 *
 * IT NEVER COMMITS, BRANCHES OR PUSHES. Files land in the working tree and
 * that is the end of it; taking that through a branch and a pull request stays
 * a human's job.
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { CliError, ExitCode } from "../util/errors.js";
import { isRepo, workingTreeState } from "../util/git.js";
import { copyRendered } from "./install.js";
import { inScope, type Manifest, type Role, roleFor } from "./manifest.js";
import {
  isSyncable,
  readStamp,
  repoPathFor,
  type Substitutions,
  substitute,
  walkFiles
} from "./render.js";

/** What happened to one template file. */
export interface DriftEntry {
  path: string;
  role: Role;
  state: "same" | "differs" | "absent";
  /** The rendered template bytes — what `--apply` would write, for a managed file. */
  rendered: string;
  /** The repo's bytes, when it has the file. */
  current?: string;
}

export interface DriftReport {
  entries: DriftEntry[];
  /** Files whose role is `managed` and which differ or are absent. */
  writable: DriftEntry[];
  /** Counts by role, for the one-line summary. */
  counts: Record<Role, number>;
  /** Files skipped because they describe a workspace that lives elsewhere. */
  outOfScope: string[];
}

export interface SyncOptions {
  templateDir: string;
  repoDir: string;
  manifest: Manifest;
  subs: Substitutions;
  /** The workspace path relative to the repo root, or `undefined` when none. */
  workspaceRel: string | undefined;
}

/**
 * Compare the repo against a re-render of the template.
 *
 * Computes and decides nothing about what to do; the caller reports and the
 * caller applies. Split out so the report and the apply walk the same tree in
 * the same order and cannot come to different conclusions about it — a report
 * that named eight files and an apply that wrote nine is the failure this
 * shape removes.
 */
export function computeDrift(opts: SyncOptions): DriftReport {
  const entries: DriftEntry[] = [];
  const outOfScope: string[] = [];
  const counts: Record<Role, number> = { managed: 0, scaffold: 0, mixed: 0, unmatched: 0 };

  for (const templateRel of walkFiles(opts.templateDir)) {
    const repoRel = repoPathFor(templateRel);

    // A template file describing the Oxy WORKSPACE describes nothing at the
    // root of a repo that keeps its workspace in `oxy/` — comparing it there
    // would report a file this repo will never have, on every run, forever.
    if (!inScope(opts.manifest, repoRel, opts.workspaceRel)) {
      outOfScope.push(repoRel);
      continue;
    }

    const role = roleFor(opts.manifest, repoRel);
    // `unmatched` is the customer's. The walk visits it because it is in the
    // template, but nothing is reported and nothing is written.
    if (role === "unmatched") continue;
    counts[role] += 1;

    const rendered = substitute(
      readFileSync(join(opts.templateDir, templateRel), "utf8"),
      opts.subs
    );
    const target = join(opts.repoDir, repoRel);

    if (!existsSync(target)) {
      entries.push({ path: repoRel, role, state: "absent", rendered });
      continue;
    }
    const current = readFileSync(target, "utf8");
    entries.push({
      path: repoRel,
      role,
      state: current === rendered ? "same" : "differs",
      rendered,
      current
    });
  }

  return {
    entries,
    // ONLY `managed`. A `scaffold` file is the customer's from the moment it
    // landed and a `mixed` one holds their bytes beside ours — update reports
    // both and rewrites neither. `oxyc adopt` is the only command that may
    // install a missing `mixed` file, and only because an imported repo never
    // had one to protect.
    writable: entries.filter((e) => e.role === "managed" && e.state !== "same"),
    counts,
    outOfScope
  };
}

/**
 * Refuse to write into a repo oxyc cannot prove it wrote.
 *
 * The proof is `.oxyc-template.json`. Every imported repo has none, so every
 * imported repo is refused — correct, and also terminal, which is exactly why
 * `oxyc adopt` exists as the one explicit act that ends it.
 */
export function requireSyncable(repoDir: string): void {
  const stamp = readStamp(repoDir);
  if (!stamp) {
    throw new CliError(
      `${repoDir} carries no ${"`.oxyc-template.json`"}, so oxyc cannot prove it rendered it`,
      {
        code: ExitCode.REFUSED,
        hint: "oxyc adopt <customer> --apply   — the one-time act that makes an imported repo syncable"
      }
    );
  }
  if (!isSyncable(repoDir)) {
    throw new CliError(`${repoDir} is marked as imported, so oxyc will not rewrite its files`, {
      code: ExitCode.REFUSED,
      detail: `provenance: ${stamp.provenance}`,
      hint: "somebody wrote that down on purpose — `oxyc adopt` is the path, if it should be syncable"
    });
  }
}

/**
 * Refuse to apply while the TEMPLATE tree has uncommitted changes.
 *
 * The stamp an apply writes names the commit it synced from. With a dirty
 * template that commit does not describe the bytes that were copied, so the
 * stamp would be a lie — and a later drift report computed against it would be
 * confidently wrong.
 */
export function requireCleanTemplate(templateRoot: string): void {
  if (!isRepo(templateRoot)) return;
  const state = workingTreeState(templateRoot);
  if (!state.dirty && state.untracked.length === 0) return;
  throw new CliError("the template tree has uncommitted changes", {
    code: ExitCode.REFUSED,
    detail: [...state.modified, ...state.untracked]
      .slice(0, 10)
      .map((p) => `  ${p}`)
      .join("\n"),
    hint: "commit them first — otherwise the stamp would name a commit whose contents were not what got copied"
  });
}

/**
 * Write the managed files.
 *
 * NEVER RE-WALKS: `report` has already decided, so the files written are
 * exactly the files it named. A report that named eight and an apply that
 * wrote nine is the failure the split exists to remove.
 *
 * Delegates the writing itself to `copyRendered`, which `oxyc adopt` also
 * uses — one definition of "install", so the two commands cannot come to
 * write files differently.
 */
export function applyDrift(opts: SyncOptions, report: DriftReport): string[] {
  return copyRendered(
    opts,
    report.writable.map((entry) => entry.path)
  );
}

/**
 * A capped unified-ish diff of one file.
 *
 * A MIXED file is shown as a DIFF, not just named: it is the role the tool
 * refuses to write and hands back to you, so "differs, yours to merge" without
 * "merge what?" has stopped one step short. Capped because an uncapped diff of
 * a lockfile is the noise that teaches people to skim.
 */
export function renderDiff(entry: DriftEntry, limit: number): string[] {
  if (entry.current === undefined) return [];
  const template = entry.rendered.split("\n");
  const yours = entry.current.split("\n");
  const out: string[] = [];
  const max = Math.max(template.length, yours.length);
  for (let i = 0; i < max; i++) {
    const t = template[i];
    const y = yours[i];
    if (t === y) continue;
    if (t !== undefined) out.push(`  TEMPLATE  ${t}`);
    if (y !== undefined) out.push(`  YOURS     ${y}`);
    if (limit > 0 && out.length >= limit) {
      out.push(`  … capped at ${limit} lines (OXYC_DIFF_LINES=0 to remove the cap)`);
      break;
    }
  }
  return out;
}

/** The diff cap. `OXYC_DIFF_LINES=0` removes it. */
export function diffLimit(): number {
  const raw = process.env.OXYC_DIFF_LINES;
  if (raw === undefined) return 40;
  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : 40;
}
