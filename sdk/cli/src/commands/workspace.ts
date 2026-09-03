/**
 * `oxyc doctor`, `oxyc update`, `oxyc adopt` — the three commands that read a
 * customer repo and, for two of them, write into it.
 *
 * They share one walk (`computeDrift` / `planAdopt`) so that what `doctor`
 * reports, what `update` would write, and what `adopt` would install cannot
 * come to different conclusions about the same repo.
 */

import { existsSync } from "node:fs";

import type { Context } from "../context/resolve.js";
import { dossierPath, isCloned } from "../customer/dossier.js";
import { memoryDir, resolveWorkspace } from "../customer/workspace.js";
import { customersOrg, displayName, listCustomers, resolveCustomer } from "../github/customers.js";
import {
  ADOPT_GENERATED_BY,
  assertAdoptable,
  planAdopt,
  refuseOnCollision
} from "../template/adopt.js";
import { copyRendered } from "../template/install.js";
import { manifestPath, templateDir, templateRepoRoot } from "../template/locate.js";
import { loadManifest } from "../template/manifest.js";
import { generatedBy, readStamp, writeStamp } from "../template/render.js";
import {
  applyDrift,
  computeDrift,
  diffLimit,
  renderDiff,
  requireCleanTemplate,
  requireSyncable
} from "../template/sync.js";
import * as log from "../ui/log.js";
import { heading } from "../ui/render.js";
import { out } from "../ui/tty.js";
import { CliError, ExitCode } from "../util/errors.js";

/** Everything the three commands need about one customer's repo. */
interface RepoView {
  name: string;
  display: string;
  slug: string;
  repoDir: string;
  cloned: boolean;
  workspaceRel: string | undefined;
}

function viewFor(name: string, refresh: boolean | undefined): RepoView {
  const customer = resolveCustomer(name, { refresh });
  const slug = `${customersOrg()}/${customer.name}`;
  const repoDir = dossierPath(slug);
  const cloned = isCloned(slug);
  return {
    name: customer.name,
    display: displayName(customer),
    slug,
    repoDir,
    cloned,
    // Resolution can throw on an ambiguous repo; that is a fact `doctor` wants
    // to report rather than crash on, so it is caught per caller.
    workspaceRel: cloned ? resolveWorkspace(repoDir) : undefined
  };
}

function requireClone(view: RepoView): void {
  if (view.cloned) return;
  throw new CliError(`${view.slug} is not cloned here`, {
    code: ExitCode.NOT_FOUND,
    hint: `gh repo clone ${view.slug} ${view.repoDir}`
  });
}

function substitutionsFor(view: RepoView) {
  return {
    slug: view.name,
    name: view.display,
    workspace: view.workspaceRel ?? "."
  };
}

// ── doctor ──────────────────────────────────────────────────────────────────

/**
 * What the tool knows about a customer, said out loud, having written nothing.
 *
 * READ-ONLY IS THE WHOLE CONTRACT. There is no `--fix`, and deliberately no
 * path here that creates so much as a directory — `memoryDir` is called rather
 * than `ensureMemoryDir`, drift is computed without applying, and no stamp is
 * written. `update --apply` and `adopt --apply` do the writing; this exists so
 * somebody can see what they would do before they do it.
 */
export function runDoctor(ctx: Context, name: string | undefined, flags: { all?: boolean }): void {
  const names = flags.all
    ? listCustomers({ refresh: ctx.flags.refresh }).map((c) => c.name)
    : [requireName(name)];

  for (const each of names) {
    reportOne(ctx, each);
  }
}

function requireName(name: string | undefined): string {
  if (name) return name;
  throw new CliError("doctor needs a customer, or --all", {
    code: ExitCode.USAGE,
    hint: "oxyc doctor <customer>   |   oxyc doctor --all"
  });
}

function reportOne(ctx: Context, name: string): void {
  let view: RepoView;
  try {
    view = viewFor(name, ctx.flags.refresh);
  } catch (cause) {
    process.stdout.write(`${heading(name)}\n  ${out.red((cause as Error).message)}\n`);
    return;
  }

  const lines: string[] = [
    `  repo         ${view.slug}`,
    `  display      ${view.display}`,
    `  clone        ${view.cloned ? view.repoDir : out.yellow("not here")}`
  ];

  if (!view.cloned) {
    lines.push(`  ${out.dim(`gh repo clone ${view.slug} ${view.repoDir}`)}`);
    process.stdout.write(`${heading(name)}\n${lines.join("\n")}\n`);
    return;
  }

  // A workspace in a SUBDIRECTORY is reported as a FACT, not a fault: the
  // platform's registered subdirectory was fixed at onboarding and nothing
  // re-opens that field, so there is nothing here to correct.
  lines.push(
    `  workspace    ${
      view.workspaceRel === undefined
        ? out.yellow("none — this repo holds no Oxy workspace")
        : view.workspaceRel === "."
          ? "the repo root"
          : `${view.workspaceRel}/  (subdirectory — registered at onboarding, do not flatten)`
    }`
  );
  lines.push(
    `  memory       ${existsSync(memoryDir(view.repoDir)) ? "present" : "absent (a launch would create it)"}`
  );

  const stamp = readStamp(view.repoDir);
  const by = generatedBy(view.repoDir);
  lines.push(
    `  provenance   ${stamp ? `${stamp.provenance}${by ? ` (by ${by})` : ""}` : out.yellow("imported — no stamp")}`
  );

  const manifest = loadManifest(manifestPath());
  const subs = substitutionsFor(view);

  if (stamp && stamp.syncable !== false && stamp.provenance !== "imported") {
    const drift = computeDrift({
      templateDir: templateDir(),
      repoDir: view.repoDir,
      manifest,
      subs,
      workspaceRel: view.workspaceRel
    });
    const differs = drift.entries.filter((e) => e.state !== "same");
    lines.push(`  update       syncable — ${describeDrift(differs)}`);
  } else {
    // For a repo `update` refuses, the ADOPTION GAP is the useful answer:
    // which files `adopt --apply` would install and this repo lacks.
    const plan = planAdopt(
      {
        templateDir: templateDir(),
        repoDir: view.repoDir,
        manifest,
        subs,
        workspaceRel: view.workspaceRel
      },
      by === ADOPT_GENERATED_BY
    );
    const gap = plan.install.length + plan.installMixed.length;
    lines.push(
      `  update       refused (no proof oxyc wrote it) — adoption gap: ${gap} file(s)` +
        (plan.collisions.length ? `, ${plan.collisions.length} collision(s)` : "")
    );
    for (const path of [...plan.install, ...plan.installMixed].slice(0, 12)) {
      lines.push(`               ${out.dim(`missing  ${path}`)}`);
    }
  }

  process.stdout.write(`${heading(name)}\n${lines.join("\n")}\n`);
}

function describeDrift(differs: { role: string }[]): string {
  if (differs.length === 0) return "in line with the template";
  const byRole = differs.reduce<Record<string, number>>((acc, e) => {
    acc[e.role] = (acc[e.role] ?? 0) + 1;
    return acc;
  }, {});
  return Object.entries(byRole)
    .map(([role, count]) => `${count} ${role}`)
    .join(", ");
}

// ── update ──────────────────────────────────────────────────────────────────

export interface UpdateFlags {
  apply?: boolean;
  diffAll?: boolean;
}

/**
 * Report how far a customer's repo has drifted from the template, and with
 * `--apply` rewrite the files oxyc owns.
 *
 * A MIXED file is shown as a DIFF, not just named: it is the role the tool
 * refuses to write and hands back to you, so "differs, yours to merge" without
 * "merge what?" has stopped one step short.
 */
export function runUpdate(ctx: Context, name: string, flags: UpdateFlags): void {
  const view = viewFor(name, ctx.flags.refresh);
  requireClone(view);
  requireSyncable(view.repoDir);

  const opts = {
    templateDir: templateDir(),
    repoDir: view.repoDir,
    manifest: loadManifest(manifestPath()),
    subs: substitutionsFor(view),
    workspaceRel: view.workspaceRel
  };
  const drift = computeDrift(opts);
  const differs = drift.entries.filter((e) => e.state !== "same");

  process.stdout.write(`${heading(`${view.name} — ${view.repoDir}`)}\n`);

  if (differs.length === 0) {
    process.stdout.write("  in line with the template\n");
    return;
  }

  const limit = diffLimit();
  for (const entry of differs) {
    const label = entry.state === "absent" ? "MISSING " : "DIFFERS ";
    const style = entry.role === "managed" ? out.yellow : out.dim;
    process.stdout.write(`  ${style(label)}${entry.role.padEnd(9)}${entry.path}\n`);
    // scaffold drift stays a one-liner: `config.yml` and `README.md` are
    // customised the day they are created, so that drift is expected and
    // permanent, and diffing it every run is the noise that teaches people to
    // skim. `--diff-all` is how you read what `--apply` is about to overwrite.
    const worthDiffing = flags.diffAll || entry.role === "mixed";
    if (worthDiffing && entry.state === "differs") {
      for (const line of renderDiff(entry, limit)) process.stdout.write(`  ${line}\n`);
    }
  }

  if (!flags.apply) {
    process.stdout.write(
      `\n  ${out.dim(`${drift.writable.length} managed file(s) would be rewritten by \`oxyc update ${name} --apply\``)}\n`
    );
    return;
  }

  requireCleanTemplate(templateRepoRoot());
  const written = applyDrift(opts, drift);
  // Re-point the stamp at the commit that was just synced from, so the next
  // run measures from a known point.
  writeStamp(view.repoDir, templateRepoRoot(), "oxyc update", new Date().toISOString());
  process.stdout.write(`\n  ${out.green(`wrote ${written.length} file(s)`)}\n`);
  log.info("nothing was committed — the change rides your pull request like any other");
}

// ── adopt ───────────────────────────────────────────────────────────────────

/**
 * Give an IMPORTED repo the managed files, then stamp it so `update` works.
 *
 * Report is the default and writing is the flag, as with `update`, and for the
 * same reason.
 */
export function runAdopt(ctx: Context, name: string, flags: { apply?: boolean }): void {
  const view = viewFor(name, ctx.flags.refresh);
  requireClone(view);

  if (view.workspaceRel === undefined) {
    // The managed files are machinery FOR a workspace. Installing CI that
    // compiles a workspace into a repo that has none would land a workflow
    // that can only fail.
    throw new CliError(`${view.slug} holds no Oxy workspace`, {
      code: ExitCode.REFUSED,
      hint: "the managed files are machinery for a workspace — there is nothing here for them to act on"
    });
  }

  const { completing } = assertAdoptable(view.repoDir);
  const opts = {
    templateDir: templateDir(),
    repoDir: view.repoDir,
    manifest: loadManifest(manifestPath()),
    subs: substitutionsFor(view),
    workspaceRel: view.workspaceRel
  };
  const plan = planAdopt(opts, completing);

  process.stdout.write(`${heading(`${view.name} — ${view.repoDir}`)}\n`);
  refuseOnCollision(plan, view.slug);

  if (plan.install.length === 0 && plan.installMixed.length === 0) {
    process.stdout.write(`  ${out.green("nothing left to install")}\n`);
    return;
  }

  for (const path of plan.install)
    process.stdout.write(`  ${out.yellow("WOULD ADD")} managed  ${path}\n`);
  for (const path of plan.installMixed)
    process.stdout.write(`  ${out.yellow("WOULD ADD")} mixed    ${path}\n`);
  if (plan.alreadyHere.length > 0) {
    process.stdout.write(
      `  ${out.dim(`${plan.alreadyHere.length} managed file(s) an earlier adopt already landed`)}\n`
    );
  }
  if (plan.installMixed.length > 0) {
    // The rule that just operated, said out loud — without it, `WOULD ADD
    // package.json` sitting in a column of managed files contradicts the
    // manifest, which says a mixed file is never written.
    process.stdout.write(
      `  ${out.dim("a MIXED file is installed only because this repo does not have it; one that is present is never touched")}\n`
    );
  }

  if (!flags.apply) {
    process.stdout.write(`\n  ${out.dim(`oxyc adopt ${name} --apply   to install them`)}\n`);
    return;
  }

  const written = copyRendered(opts, [...plan.install, ...plan.installMixed]);
  // The stamp is left exactly where it is on a completing run: this repo's
  // claim is already written, and re-stamping would move the point a later
  // drift report measures from for no reason.
  if (!completing) {
    writeStamp(view.repoDir, templateRepoRoot(), ADOPT_GENERATED_BY, new Date().toISOString());
  }
  process.stdout.write(`\n  ${out.green(`installed ${written.length} file(s)`)}\n`);
  process.stdout.write(
    `  ${out.dim("publish.yaml discovers apps under `apps/` only — an adopted repo that keeps them elsewhere gets a workflow that finds nothing (a safe no-op)")}\n`
  );
  log.info("nothing was committed — the change rides your pull request like any other");
}
