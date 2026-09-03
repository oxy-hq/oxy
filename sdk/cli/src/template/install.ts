/**
 * Landing one rendered template file into a repo.
 *
 * ONE IMPLEMENTATION, TWO CALLERS. `oxyc update --apply` and `oxyc adopt
 * --apply` both write template files, and if each had its own copy the two
 * would eventually disagree — an installed file that skipped the render, or
 * lost its exec bit, is a second definition of "install" that only one of the
 * two paths is ever tested through. That is not hypothetical: it is how a live
 * repo ended up with a `scripts/dev.sh` that had landed but could not run.
 */

import { chmodSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { Manifest } from "./manifest.js";
import { type Substitutions, substitute, templateSourceFor } from "./render.js";

export interface InstallOptions {
  templateDir: string;
  repoDir: string;
  manifest: Manifest;
  subs: Substitutions;
  workspaceRel: string | undefined;
}

/**
 * Write the named files, rendered.
 *
 * RENDERED, NEVER COPIED: `__NAME__`, `__SLUG__` and `__WORKSPACE__` are
 * placeholders, and a plain copy writes the literal text into a live customer
 * repo. `__WORKSPACE__` is the one that matters most here — this is the path
 * that installs into a repo whose workspace is NOT the root, and a CI workflow
 * that compiles `__WORKSPACE__` is a workflow that can only fail.
 */
export function copyRendered(opts: InstallOptions, repoPaths: string[]): string[] {
  const written: string[] = [];
  for (const repoRel of repoPaths) {
    const source = join(opts.templateDir, templateSourceFor(repoRel));
    const target = join(opts.repoDir, repoRel);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, substitute(readFileSync(source, "utf8"), opts.subs));
    try {
      chmodSync(target, statSync(source).mode & 0o777);
    } catch {
      // A filesystem without POSIX modes. The content landed, which is the
      // part that matters; a missing exec bit is reported by `doctor`.
    }
    written.push(repoRel);
  }
  return written;
}
