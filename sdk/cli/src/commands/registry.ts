/**
 * `oxyc new`, `oxyc import`, `oxyc remove` — the three doors into and out of
 * the customer registry.
 *
 * REGISTRATION IS TAGGING A REPO and nothing else. `import` adds the topic and
 * stops; `new` creates the repo already tagged; `remove` drops the topic. There
 * is no customer file anywhere, which is why there is nothing else to keep in
 * sync.
 */

import { existsSync, rmSync } from "node:fs";

import type { Context } from "../context/resolve.js";
import { dossierPath, isCloned } from "../customer/dossier.js";
import { customersOrg, invalidateCache, resolveCustomer } from "../github/customers.js";
import { ghAvailable } from "../github/gh.js";
import {
  addTopic,
  createRepo,
  inspectRepo,
  refuseInconclusive,
  removeTopic
} from "../github/topics.js";
import { NEW_GENERATED_BY } from "../template/adopt.js";
import { templateDir, templateRepoRoot } from "../template/locate.js";
import { renderTemplate, writeStamp } from "../template/render.js";
import * as log from "../ui/log.js";
import { out } from "../ui/tty.js";
import { CliError, ExitCode } from "../util/errors.js";
import { clone, currentBranch, isRepo, workingTreeState } from "../util/git.js";

/**
 * Create a new customer workspace repo, tagged, scaffolded and pushed-ready.
 *
 * The order is deliberate: PROVE THE REPO DOES NOT EXIST, create it, render,
 * stamp. An inconclusive existence check refuses outright — reading "GitHub is
 * having a moment" as "no such repo" is how this command would create over a
 * live customer's repository.
 */
export function runNew(_ctx: Context, name: string, flags: { display?: string }): void {
  const org = customersOrg();
  const slug = `${org}/${name}`;

  const existence = inspectRepo(slug);
  if (existence.kind === "inconclusive") throw refuseInconclusive(slug, existence.why);
  if (existence.kind === "exists") {
    throw new CliError(`${slug} already exists`, {
      code: ExitCode.REFUSED,
      hint: `oxyc import ${slug}   — to register a repo that is already there`
    });
  }

  const display = flags.display ?? name;
  const dest = dossierPath(slug);
  if (existsSync(dest)) {
    throw new CliError(`${dest} already exists`, {
      code: ExitCode.REFUSED,
      hint: "move it aside — scaffolding into it could overwrite work"
    });
  }

  log.info(`creating ${slug}`);
  createRepo(slug, display);

  log.info(`rendering the workspace template into ${dest}`);
  const written = renderTemplate(templateDir(), dest, {
    slug: name,
    name: display,
    // A freshly rendered tree IS a workspace at its root — any other value
    // would describe a repo this command cannot create.
    workspace: "."
  });
  writeStamp(dest, templateRepoRoot(), NEW_GENERATED_BY, new Date().toISOString());
  invalidateCache(org);

  process.stdout.write(
    `${out.green(`created ${slug}`)}\n` +
      `  ${written.length + 1} files scaffolded at ${dest}\n` +
      `  ${out.dim("nothing was committed or pushed — `git add -A && git commit && git push` when it looks right")}\n`
  );
}

/**
 * Register an EXISTING repo as a customer workspace.
 *
 * It never scaffolds: the repo has content already, and dropping the template
 * over it is what `oxyc adopt` exists to do carefully and on purpose.
 */
export function runImport(_ctx: Context, slug: string, flags: { clone?: boolean }): void {
  if (!slug.includes("/")) {
    throw new CliError(`import needs <org>/<repo>, got "${slug}"`, {
      code: ExitCode.USAGE,
      hint: `oxyc import ${customersOrg()}/${slug}`
    });
  }

  const existence = inspectRepo(slug);
  if (existence.kind === "inconclusive") throw refuseInconclusive(slug, existence.why);
  if (existence.kind === "absent") {
    throw new CliError(`${slug} does not exist`, {
      code: ExitCode.NOT_FOUND,
      hint: `oxyc new ${slug.split("/")[1]}   — to create it`
    });
  }

  addTopic(slug);
  process.stdout.write(`${out.green(`registered ${slug}`)}\n`);

  const path = dossierPath(slug);
  if (isCloned(slug)) return;
  if (flags.clone) {
    log.info(`cloning ${slug} → ${path}`);
    clone(slug, path, ghAvailable());
    return;
  }
  process.stdout.write(
    `  ${out.dim(`not cloned here — ${ghAvailable() ? `gh repo clone ${slug} ${path}` : `git clone https://github.com/${slug}.git ${path}`}`)}\n`
  );
}

/**
 * Drop the topic. The REPO IS NEVER DELETED — not by this, and not by
 * `--purge`, which deletes only the local clone. A customer removed by mistake
 * is re-registered with `oxyc import`.
 */
export function runRemove(
  ctx: Context,
  name: string,
  flags: { purge?: boolean; yes?: boolean }
): void {
  const customer = resolveCustomer(name, { refresh: ctx.flags.refresh });
  const slug = `${customersOrg()}/${customer.name}`;

  removeTopic(slug);
  process.stdout.write(
    `${out.green(`unregistered ${slug}`)} ${out.dim("(the repo itself is untouched)")}\n`
  );

  if (!flags.purge) return;

  const path = dossierPath(slug);
  if (!existsSync(path)) {
    log.info("no local clone to purge");
    return;
  }

  // REFUSES IF THE CLONE HOLDS ANYTHING THAT EXISTS NOWHERE ELSE. `--purge` is
  // a convenience for reclaiming disk, and a convenience must not be able to
  // destroy the only copy of something.
  const blockers = purgeBlockers(path);
  if (blockers.length > 0) {
    throw new CliError(`${path} holds work that exists nowhere else`, {
      code: ExitCode.REFUSED,
      detail: blockers.map((b) => `  ${b}`).join("\n"),
      hint: "push or discard it, then run `oxyc remove --purge` again"
    });
  }

  // Never assumes yes: with no terminal to ask, an unconfirmed destructive
  // operation refuses rather than proceeding or hanging on a prompt nobody
  // can answer.
  if (!flags.yes && !process.stdin.isTTY) {
    throw new CliError("--purge needs --yes when stdin is not a terminal", {
      code: ExitCode.REFUSED,
      hint: `oxyc remove ${name} --purge --yes`
    });
  }

  rmSync(path, { recursive: true, force: true });
  process.stdout.write(`${out.green(`deleted the local clone at ${path}`)}\n`);
}

/** Everything in a clone that would be lost by deleting it. */
function purgeBlockers(path: string): string[] {
  const blockers: string[] = [];
  if (!isRepo(path)) return ["not a git repository — oxyc cannot tell what would be lost"];

  const state = workingTreeState(path);
  if (state.modified.length > 0) blockers.push(`${state.modified.length} uncommitted change(s)`);
  if (state.untracked.length > 0) blockers.push(`${state.untracked.length} untracked file(s)`);

  const branch = currentBranch(path);
  if (!branch) blockers.push("detached HEAD");

  return blockers;
}
