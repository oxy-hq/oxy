/**
 * `oxyc skills install` / `oxyc skills list` — put this package's Claude skills
 * where every session will load them.
 *
 * THIS IS THE ONE PLACE THE PORT CHANGES SHAPE rather than translating.
 *
 * The bash tooling symlinked `skills/` OUT of a git clone that had to persist
 * forever: moving or deleting the clone broke `oxyc` and left dangling links
 * in `~/.claude/skills/`, and a `git pull` that ADDED a skill created no link
 * for it and printed no warning, so it silently did not load until somebody
 * remembered to re-run `install.sh`. `oxyc self-update` existed almost
 * entirely to remember that step.
 *
 * As an npm package the skills ship INSIDE the install, so the whole class
 * goes away: `pnpm add -g @oxy-hq/cli` replaces the pull, and this command
 * re-points the links at whatever version is now installed. What is kept is
 * the part that was actually load-bearing — never clobbering a name that
 * something else owns, and exiting non-zero when a skill could not be linked.
 * A skill that did not link is never reported as fine.
 */

import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readlinkSync,
  rmSync,
  symlinkSync
} from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { REINSTALL_REMEDY } from "../template/embedded.js";
import { skillsDir } from "../template/locate.js";
import * as log from "../ui/log.js";
import { table } from "../ui/render.js";
import { out } from "../ui/tty.js";
import { CliError, ExitCode, refusal } from "../util/errors.js";

/** Where Claude Code loads global skills from. */
function targetDir(): string {
  return process.env.OXYC_SKILLS_TARGET ?? join(homedir(), ".claude", "skills");
}

type LinkState = "linked" | "relinked" | "already" | "blocked";

interface SkillResult {
  name: string;
  state: LinkState;
  detail: string;
}

/** The skills this package ships. */
function shipped(): string[] {
  const dir = skillsDir();
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();
}

/**
 * Link one skill, refusing to clobber a name something else owns.
 *
 * A foreign symlink or a real directory at that name is left ALONE and
 * reported — the bash installer did the same, and the difference is that its
 * "skipped" line was one nobody read. Here it makes the command exit non-zero.
 */
function linkOne(name: string, source: string, target: string): SkillResult {
  const dest = join(target, name);
  if (!existsSync(dest) && !isSymlink(dest)) {
    symlinkSync(source, dest, "dir");
    return { name, state: "linked", detail: source };
  }
  if (isSymlink(dest)) {
    const current = readlinkSync(dest);
    if (current === source) return { name, state: "already", detail: source };
    // A link pointing at an OLD install of this package is ours to move; one
    // pointing anywhere else is not.
    if (current.includes(`${"@oxy-hq"}/cli`) || current.includes("customer-tooling")) {
      rmSync(dest);
      symlinkSync(source, dest, "dir");
      return { name, state: "relinked", detail: `was ${current}` };
    }
    return { name, state: "blocked", detail: `a symlink to ${current} is in the way` };
  }
  return { name, state: "blocked", detail: "a real directory is in the way" };
}

function isSymlink(path: string): boolean {
  try {
    return lstatSync(path).isSymbolicLink();
  } catch {
    return false;
  }
}

/** Link every shipped skill into the user's global skills directory. */
export function runSkillsInstall(): void {
  const source = skillsDir();
  const target = targetDir();
  const names = shipped();

  if (names.length === 0) {
    throw new CliError("this installation ships no skills", {
      code: ExitCode.FAILURE,
      remedy: REINSTALL_REMEDY
    });
  }

  mkdirSync(target, { recursive: true });
  const results = names.map((name) => linkOne(name, join(source, name), target));

  process.stdout.write(
    `${table(results, [
      { header: "SKILL", value: (r) => r.name },
      { header: "STATE", value: (r) => (r.state === "blocked" ? out.red(r.state) : r.state) },
      { header: "DETAIL", value: (r) => r.detail }
    ])}\n`
  );

  const blocked = results.filter((r) => r.state === "blocked");
  if (blocked.length === 0) return;

  // NON-ZERO, deliberately. A skill that was not linked will not load, and a
  // command that reported that as success is how the gap stayed invisible.
  throw refusal(`${blocked.length} skill(s) could not be linked`, {
    detail: blocked.map((r) => `  ${r.name}: ${r.detail}`).join("\n"),
    remedy: `move what is in the way under ${target}, then run \`oxyc skills install\` again`
  });
}

/** What this package ships, and whether each is currently linked. */
export function runSkillsList(): void {
  const source = skillsDir();
  const target = targetDir();
  const rows = shipped().map((name) => {
    const dest = join(target, name);
    const linked = isSymlink(dest) && readlinkSync(dest) === join(source, name);
    return { name, linked };
  });

  if (rows.length === 0) {
    // SAME SENTENCE AS THE THROW ABOVE, so it gets the same remedy — a listing
    // that finds nothing is not a failure (exit 0 stands), but a reader who
    // sees this and the thrown version has no reason to be told what to do in
    // only one of them.
    log.warn("this installation ships no skills");
    log.remedy(REINSTALL_REMEDY);
    return;
  }

  process.stdout.write(
    `${table(rows, [
      { header: "SKILL", value: (r) => r.name },
      { header: "LINKED", value: (r) => (r.linked ? "yes" : out.yellow("no")) }
    ])}\n`
  );
}
