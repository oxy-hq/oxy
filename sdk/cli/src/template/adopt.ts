/**
 * `oxyc adopt <customer>` — install the managed files an IMPORTED repo lacks,
 * then stamp it, so `oxyc update` works on it from then on.
 *
 * THE ONE-WAY DOOR THIS OPENS. `oxyc update` may only rewrite a repo it can
 * prove it wrote, and the proof is the stamp. Every imported repo has none, so
 * every imported repo is refused forever — correct, and also terminal: the
 * live imported repos are each missing eight managed files and under that rule
 * could never receive the template's CI or dev tooling at all. `doctor` names
 * the gap; this closes it.
 *
 * A SEPARATE COMMAND AND NOT A FLAG ON `update`: `--adopt` would sit one
 * keystroke from the flag people type on a repo oxyc really did scaffold, and
 * its whole effect is to make a repo writable by a command that then writes to
 * it without asking.
 *
 * WHAT IT MAY TOUCH, and the argument is entirely the manifest's:
 *
 *   managed    YES. CI workflows, the scripts they call, `.gitattributes`, the
 *              stamp. Every one is repo-relative and belongs at the ROOT in
 *              both layouts, so installing them cannot move or mirror the
 *              workspace.
 *   scaffold   NO. Those are the WORKSPACE's files, and an imported repo has
 *              its own inside `oxy/`, written before oxyc ever saw the repo.
 *   mixed      ONLY WHEN THE REPO DOES NOT HAVE IT — see `mixedPlan` below.
 *   unmatched  NO, by construction: the walk is over the TEMPLATE.
 */

import { existsSync } from "node:fs";
import { join } from "node:path";

import { CliError, ExitCode } from "../util/errors.js";
import { inScope, type Manifest, roleFor } from "./manifest.js";
import { generatedBy, readStamp, repoPathFor, type Substitutions, walkFiles } from "./render.js";

/** The command name recorded in a stamp this writes. */
export const ADOPT_GENERATED_BY = "oxyc adopt";
/** The command name `oxyc new` records — the value that tells the two apart. */
export const NEW_GENERATED_BY = "oxyc new";

export interface AdoptPlan {
  /** Managed files the repo lacks — installed by `--apply`. */
  install: string[];
  /** Mixed files the repo lacks — also installed; see the note below. */
  installMixed: string[];
  /** Managed paths that already exist and are NOT this command's own output. */
  collisions: string[];
  /** Managed files an earlier run of THIS command already landed. */
  alreadyHere: string[];
  /** Skipped because they describe a workspace that lives elsewhere. */
  outOfScope: string[];
}

export interface AdoptOptions {
  templateDir: string;
  repoDir: string;
  manifest: Manifest;
  subs: Substitutions;
  workspaceRel: string | undefined;
}

/**
 * Decide whether this repo may be adopted at all.
 *
 * IT FINISHES ITS OWN WORK. A repo whose stamp says `oxyc adopt` wrote it is
 * not somebody else's repo to refuse — it is THIS command's output, and a file
 * still missing from it is a file only this command installs, since `update`
 * reports a missing mixed file and never restores it. An adopt that refused
 * its own output is how a live repo ended up holding `scripts/dev.sh` and no
 * `package.json`, reachable by neither command.
 */
export function assertAdoptable(repoDir: string): { completing: boolean } {
  const stamp = readStamp(repoDir);
  if (!stamp) return { completing: false };

  const by = generatedBy(repoDir);
  if (by === ADOPT_GENERATED_BY) return { completing: true };

  if (by === NEW_GENERATED_BY) {
    throw new CliError(`${repoDir} was scaffolded by \`oxyc new\``, {
      code: ExitCode.REFUSED,
      hint: "`oxyc update` is its path — adopt is for a repo oxyc did not create"
    });
  }
  // A hand-written `provenance: "imported"` outranks everything: somebody
  // wrote that down on purpose.
  throw new CliError(`${repoDir} already carries a stamp oxyc did not write`, {
    code: ExitCode.REFUSED,
    detail: `generated_by: ${by ?? "(absent)"}, provenance: ${stamp.provenance}`,
    hint: "that was recorded deliberately — remove or correct the stamp if it is wrong"
  });
}

/**
 * What adopting would install, and what stands in the way.
 *
 * Reports and decides nothing; the caller does both.
 */
export function planAdopt(opts: AdoptOptions, completing: boolean): AdoptPlan {
  const plan: AdoptPlan = {
    install: [],
    installMixed: [],
    collisions: [],
    alreadyHere: [],
    outOfScope: []
  };

  for (const templateRel of walkFiles(opts.templateDir)) {
    const repoRel = repoPathFor(templateRel);

    if (!inScope(opts.manifest, repoRel, opts.workspaceRel)) {
      plan.outOfScope.push(repoRel);
      continue;
    }

    const role = roleFor(opts.manifest, repoRel);
    const present = existsSync(join(opts.repoDir, repoRel));

    if (role === "managed") {
      if (!present) {
        plan.install.push(repoRel);
      } else if (completing) {
        // Not a collision: the claim is already written and the file at that
        // path is one adopt itself installed. Refusing here would refuse the
        // completing run for exactly the files the earlier run got right.
        plan.alreadyHere.push(repoRel);
      } else {
        plan.collisions.push(repoRel);
      }
      continue;
    }

    // A MIXED file that is PRESENT is never touched: part of it is the
    // customer's and overwriting it destroys their half. A mixed file that is
    // ABSENT has no half of theirs to protect, and withholding it strands the
    // managed files it pairs with — `scripts/dev.sh` installed with no
    // `package.json` to carry the `dev` script that runs it is a tool with no
    // handle. PER FILE, not per role.
    //
    // SCOPED TO ADOPT, deliberately. `oxyc update` keeps REPORTING a missing
    // mixed file rather than restoring it: a scaffolded repo always had both
    // halves, so an absence there is a deletion somebody meant, and update
    // runs over and over where adopt is a one-time act typed on purpose.
    if (role === "mixed" && !present) plan.installMixed.push(repoRel);
  }

  return plan;
}

/**
 * A collision refuses the WHOLE adopt rather than installing the rest.
 *
 * Adopting does not merely copy files — it writes a CLAIM: this repo is a
 * render of the template, and every managed path in it is oxyc's to rewrite.
 * `oxyc update --apply` acts on that claim silently. So a partial adopt does
 * not leave the customer's file untouched; it leaves it MARKED, sitting at a
 * managed path in a repo that now reports itself syncable, for the next
 * `update --apply` — run by someone who was not here — to destroy without
 * asking.
 *
 * That is data loss deferred by one command, and deferred loss is worse than
 * immediate refusal precisely because nobody is watching when it lands. The
 * refusal is completely recoverable: rename or delete their file, or write a
 * manifest rule saying it is theirs, and run adopt again.
 *
 * The alternatives are worse in the same direction. Installing the rest with a
 * warning relies on somebody reading a warning about a consequence that
 * arrives weeks later. Installing the rest and withholding the stamp leaves a
 * repo carrying our CI that no command can ever sync — the exact state adopt
 * exists to end. Skipping the colliding path and stamping anyway is the
 * marked-file case again, with the mark hidden.
 */
export function refuseOnCollision(plan: AdoptPlan, repoLabel: string): void {
  if (plan.collisions.length === 0) return;
  throw new CliError(
    `${repoLabel} already has files at ${plan.collisions.length} managed path(s)`,
    {
      code: ExitCode.REFUSED,
      detail: plan.collisions.map((p) => `  ${p}`).join("\n"),
      hint:
        "adopt is all-or-nothing: it would mark these as oxyc's to rewrite, and a later\n" +
        "     `oxyc update --apply` would destroy them. Move them aside, or add a manifest\n" +
        "     rule saying they are the customer's, then run adopt again."
    }
  );
}
