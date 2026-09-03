/**
 * `oxyc <customer>` — start a Claude Code session scoped to one customer.
 *
 * The briefing is the product here. Everything below exists so that the prompt
 * handed to the model DESCRIBES THE SESSION IT IS ACTUALLY ATTACHED TO — three
 * repo shapes times `--here`, each saying exactly what is true of it. "Your
 * working directory IS their workspace" is false twice over: for a repo whose
 * root holds the customer's own ETL, apps and workflows with the workspace in
 * `oxy/`, and for a repo that holds no workspace at all.
 *
 * A prompt that overclaims is worse than a vague one, because the model has no
 * way to check it.
 */

import { spawn } from "node:child_process";

import type { Context } from "../context/resolve.js";
import { dossierPath, ensureCloned, isCloned } from "../customer/dossier.js";
import { attributionLine, sharedRepoContext } from "../customer/repos.js";
import { ensureMemoryDir, resolveWorkspace, workspaceDir } from "../customer/workspace.js";
import { customersOrg, displayName, resolveCustomer } from "../github/customers.js";
import * as log from "../ui/log.js";
import { ExitCode } from "../util/errors.js";

export interface LaunchFlags {
  /** Run in the current directory, granting the customer's repo via --add-dir. */
  here?: boolean;
  /** Print the command instead of running it. */
  dryRun?: boolean;
  /** Everything after `--`, handed to claude. */
  passthrough: string[];
}

export function runLaunch(ctx: Context, name: string, flags: LaunchFlags): void {
  // No fallback on a failed resolve: it is either unknown, ambiguous (it says
  // which, and lists the candidates), or `gh` could not answer at all.
  // Continuing past any of those launches a session scoped to a customer
  // nobody named.
  const customer = resolveCustomer(name, { refresh: ctx.flags.refresh });
  const slug = `${customersOrg()}/${customer.name}`;
  const repoDir = isCloned(slug) ? dossierPath(slug) : ensureCloned(slug, { autoClone: true });

  // Detected AFTER any clone, because the clone is what can bring a
  // `config.yml` into a checkout that had none.
  const workspaceRel = resolveWorkspace(repoDir);
  const workspace = workspaceDir(repoDir);

  // Created rather than merely named: this path is handed to Claude Code as
  // `autoMemoryDirectory`, and a repo oxyc did not scaffold has no `memory/`.
  const memory = ensureMemoryDir(repoDir);
  const display = displayName(customer);

  const briefing = buildBriefing({
    display,
    // The repo NAME, never the display name and never the abbreviation that
    // was typed: the display name is a description wherever one is set, so it
    // is not a resolvable identifier, and `pokehouse-oxy` and
    // `pokehouse-context` both strip to `pokehouse` — stripping manufactures
    // the one collision a full repo name cannot have.
    name: customer.name,
    repoDir,
    workspace,
    workspaceRel,
    memory,
    here: Boolean(flags.here)
  });

  const settings = JSON.stringify({ autoMemoryDirectory: memory });
  const args = flags.here
    ? [
        "--settings",
        settings,
        "--add-dir",
        repoDir,
        "--append-system-prompt",
        briefing,
        ...flags.passthrough
      ]
    : ["--settings", settings, "--append-system-prompt", briefing, ...flags.passthrough];

  if (flags.dryRun || process.env.OXYC_DRY_RUN === "1") {
    // The `cd` is printed too, because it is half of what makes the session
    // what it is — a line that omitted it would describe a launch nobody
    // performs.
    const prefix = flags.here ? "" : `cd ${repoDir} && `;
    process.stdout.write(`${prefix}claude ${args.map(shellQuote).join(" ")}\n`);
    return;
  }

  log.info(`${customer.name} → ${flags.here ? process.cwd() : repoDir}`);
  const child = spawn("claude", args, {
    cwd: flags.here ? process.cwd() : repoDir,
    stdio: "inherit",
    env: { ...process.env, OXYC_DOSSIER: repoDir }
  });
  // REPORTED HERE, not thrown. This callback runs on the event loop long after
  // `runLaunch` returned, so a throw from it unwinds to nothing: node prints an
  // uncaught-exception stack and exits 1. A machine without Claude Code
  // installed — the whole reason this branch exists — would get that stack
  // instead of the one line telling it what to install.
  child.on("error", (cause) => {
    const missing = (cause as NodeJS.ErrnoException).code === "ENOENT";
    log.error(missing ? "`claude` is not on PATH" : `could not start claude: ${cause.message}`);
    log.hint(
      missing
        ? "install Claude Code, or use `--dry-run` to print the command instead"
        : "use `--dry-run` to print the command and run it yourself"
    );
    // NOT `USAGE`. That code means "you called it wrong, stop", and an agent
    // reading it concludes its own arguments were the problem and will not act
    // on the hint it was just handed. `oxyc launch acme` on a box without
    // Claude Code installed is a CORRECT invocation in an incomplete
    // environment — `UNAVAILABLE` says exactly that, and is the code the
    // contract already documents as retryable-once-the-dependency-is-there.
    process.exit(missing ? ExitCode.UNAVAILABLE : ExitCode.FAILURE);
  });
  // The child owns the terminal from here; its exit code is ours.
  child.on("exit", (code) => process.exit(code ?? 0));
}

interface BriefingInput {
  display: string;
  name: string;
  repoDir: string;
  workspace: string | undefined;
  workspaceRel: string | undefined;
  memory: string;
  here: boolean;
}

/**
 * Where the work is, said truthfully for each of the six cases.
 *
 * Under `--here` the working directory is one of OUR repos and the customer's
 * is merely granted, so every sentence has to be re-pointed.
 */
function whereClause(input: BriefingInput): string {
  const { display, repoDir, workspace, workspaceRel } = input;
  const subdir = workspaceRel && workspaceRel !== "." ? workspaceRel : undefined;

  if (input.here) {
    if (!workspaceRel) {
      return `${display}'s repo is at ${repoDir}, which is NOT your working directory this session. It holds no Oxy workspace — no config.yml — so it is their memory and notes rather than a semantic model.`;
    }
    if (!subdir) {
      return `${display}'s Oxy workspace — semantic model, pipelines, automations, custom apps — is at ${repoDir}, which is NOT your working directory this session.`;
    }
    return `${display}'s Oxy workspace — semantic model, pipelines, automations, custom apps — is the ${subdir}/ subdirectory of their repo, at ${workspace}, which is NOT your working directory this session. Nor is the repo it sits in, at ${repoDir}; the rest of that repo, outside ${subdir}/, is the customer's own work and no part of the workspace.`;
  }

  if (!workspaceRel) {
    return `Your working directory is ${display}'s repo. It holds no Oxy workspace — there is no config.yml at its root or one level below — so treat it as their memory, notes and whatever else the team keeps for them, not as a semantic model Oxy compiles.`;
  }
  if (!subdir) {
    return `Your working directory IS ${display}'s Oxy workspace: the semantic model, pipelines, automations and custom apps under it are theirs, and editing them is customer work rather than tooling work.`;
  }
  return `Your working directory is ${display}'s repo, and it is NOT the whole of their Oxy workspace. The workspace is the ${subdir}/ subdirectory, at ${workspace}: the semantic model, pipelines, automations and custom apps under THAT directory are what Oxy compiles and serves.

Everything else in ${repoDir} — outside ${subdir}/ — is the customer's own work sitting alongside it: their own tooling, their own CI, their own apps. It is theirs to read and to change on their behalf, but it is not the workspace, and nothing about it follows the conventions oxy expects of one.

Editing either is customer work rather than tooling work.`;
}

function buildBriefing(input: BriefingInput): string {
  const subdir = input.workspaceRel && input.workspaceRel !== "." ? input.workspaceRel : undefined;

  // Worth saying out loud exactly when the repo is not the workspace —
  // otherwise a session told the workspace is `oxy/` will reasonably put
  // memory under it.
  const memoryNote = subdir
    ? `\n\nThat directory is at the REPO ROOT, deliberately outside the ${subdir}/ workspace: what you learn is about the CUSTOMER, not about their workspace, and this repo holds more than the workspace does.`
    : "";

  return `You are working on behalf of the customer ${input.display}.

${whereClause(input)}

Changes to their repo go via a branch and a pull request — memory included.
Branch per CHANGE, not per session: one session may produce several.

Some work for this customer lands in OUR repos rather than in theirs. Those are:

${sharedRepoContext()}
Prefer the checkout that is already there over cloning a second copy.

When you open a pull request in any of them — that is, in a repo that is NOT
${input.name} — put this on its own line in the body:

    ${attributionLine(input.name)}

It is the only thing that ties that pull request back to this customer:
"oxyc activity" finds the pull request by matching that exact line. Their own
repo needs no such line, because the repo is already the answer.

Durable, customer-specific facts you learn this session — their infrastructure,
their data quirks, decisions made for them, gotchas that cost time — belong in
the memory directory at ${input.memory}, which is shared with the team via git.${memoryNote}
Commit a fact deliberately, in the pull request of the change that prompted it,
rather than sweeping it up at the end of the session.

Product-level facts that are true regardless of which customer you are working
for do not belong there; they belong in the ordinary project memory.`;
}

/** Single-quote for a shell, so `--dry-run` prints something runnable. */
function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_./:@-]+$/.test(value)) return value;
  return `'${value.replaceAll("'", `'\\''`)}'`;
}
