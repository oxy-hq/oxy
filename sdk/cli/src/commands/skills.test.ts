/**
 * `oxyc skills`, driven through the real binary.
 *
 * WRITTEN BECAUSE THE REFUSAL WAS UNPINNED. `runSkillsInstall`'s blocked-skill
 * path builds a `CliError` carrying both a `detail` (the list) and a `remedy`
 * (what to do), and those two render differently on purpose — dim continuation
 * lines versus a `fix:` block set apart. Nothing asserted that, so swapping the
 * two fields type-checked and printed the list as an instruction and the
 * instruction as detail, with a green suite either way. That is exactly the
 * mis-channelling the `hint`/`remedy` split exists to prevent.
 */

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";
import { ExitCode } from "../util/errors.js";

const BIN = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "dist", "main.mjs");
const SCRATCH: string[] = [];
afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
});

/** A scratch root. `oxyc()` points the skills target inside it explicitly. */
function home(): string {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-skills-"));
  SCRATCH.push(dir);
  return dir;
}

/**
 * A skills directory with nothing in it. ONE of them, for the whole file.
 *
 * Memoized, because the first version of this helper factored the CONSTRUCTION
 * and not the directory: it called `mkdtempSync` on every invocation while its
 * own doc claimed the three cases "differ in the SUBCOMMAND and in nothing
 * else". Byte-identical contents meant nothing could observe the difference,
 * which is exactly why the claim survived being written.
 */
let EMPTY_SKILLS: string | undefined;
function emptySkills(): string {
  if (EMPTY_SKILLS === undefined) {
    EMPTY_SKILLS = mkdtempSync(join(tmpdir(), "oxyc-noskills-"));
    SCRATCH.push(EMPTY_SKILLS);
  }
  return EMPTY_SKILLS;
}

function oxyc(homeDir: string, args: string[], skillsDir?: string) {
  if (!existsSync(BIN)) throw new Error(`${BIN} missing — run \`pnpm build\``);
  const r = spawnSync(process.execPath, [BIN, "skills", ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      // Still set: on POSIX, anything reaching `homedir()` on a path this
      // suite does not override lands in the scratch root. Only on POSIX —
      // which is precisely why the line below exists, and why the two comments
      // would contradict each other without this word.
      HOME: homeDir,
      // BY CONSTRUCTION, not by `$HOME`. `targetDir()` reads this first and
      // only then `homedir()` — which prefers `$HOME` on POSIX but reads
      // `USERPROFILE` on Windows, where this suite would otherwise symlink into
      // a contributor's real `~/.claude/skills` and `SCRATCH` would not clean
      // it. The seam exists for exactly this.
      OXYC_SKILLS_TARGET: join(homeDir, ".claude", "skills"),
      ...(skillsDir ? { OXYC_SKILLS_DIR: skillsDir } : {}),
      OXY_CREDENTIALS_PATH: join(homeDir, "__no_creds__.json"),
      OXYC_CACHE_DIR: join(homeDir, "__no_cache__"),
      OXY_TOKEN: "",
      NO_COLOR: "1"
    }
  });
  return { status: r.status ?? -1, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

describe("a blocked install", () => {
  /**
   * A real directory where a link belongs cannot be replaced without deleting
   * someone's files, so the command refuses rather than choosing for them.
   *
   * REFUSED (8), not FAILURE: the operation was understood and declined. The
   * message assertions live in the case below, whose positives subsume them —
   * the verdict is what is unique here.
   */
  it("refuses rather than choosing for you", () => {
    const h = home();
    mkdirSync(join(h, ".claude", "skills", "oxy-cli"), { recursive: true });
    writeFileSync(join(h, ".claude", "skills", "oxy-cli", "notes.md"), "someone's work\n");

    const r = oxyc(h, ["install"]);
    expect(r.status).toBe(ExitCode.REFUSED);
    expect(r.stderr).toMatch(/could not be linked/);
  });

  /**
   * THE TWO FIELDS RENDER DIFFERENTLY, which is the whole reason they are two.
   * The blocked list is `detail` — dim, indented, no marker. The instruction is
   * `remedy` — a `fix:` block set apart by blank lines. Swapping them in the
   * throw type-checks, so this is what catches it.
   */
  it("prints the list as detail and the instruction as a remedy", () => {
    const h = home();
    mkdirSync(join(h, ".claude", "skills", "oxy-cli"), { recursive: true });
    writeFileSync(join(h, ".claude", "skills", "oxy-cli", "notes.md"), "x\n");

    const { stderr } = oxyc(h, ["install"]);
    // The POSITIVES do still count spaces, deliberately: a format change should
    // fail loudly at the place that describes the format. The paragraph below
    // is about the NEGATIVES, where a count going stale means silence.
    expect(stderr).toMatch(/\n {4}oxy-cli: a real directory is in the way/);
    expect(stderr).toMatch(/\n\n {2}fix: move what is in the way under /);

    // AND NEITHER WEARS THE OTHER'S CLOTHES — asserted against WHAT PRINTED,
    // not against an indent. The first spelling encoded the arithmetic and was
    // vacuous under the very swap it named: `detail` lines carry a two-space
    // prefix from `skills.ts` on top of the renderer's two while the remedy
    // carries none, so the swap renders `fix:` + THREE spaces and the
    // instruction at two rather than four, and `/fix: oxy-cli:/` and
    // `/^ {4}move/` both missed. Measured from the binary, not reasoned about.
    //
    // Widening to `\s+` and `{2,4}` fixed those two cases and left the reader
    // depending on a count. It is not the dependency an earlier version of this
    // comment claimed — the `skills.ts` prefix decorates the LIST, so a swapped
    // instruction has no prefix and renders at two spaces whatever that value
    // is — but a count is a count, and extraction needs no argument about which
    // one moves.
    const fixLine = stderr.match(/^ *fix: (.*)$/m)?.[1];
    expect(fixLine).toBeDefined();
    expect(fixLine).not.toContain("a real directory is in the way");

    // Every line that is not the remedy: the instruction must not be among them.
    const notTheRemedy = stderr
      .split("\n")
      .filter((line) => !line.includes("fix:"))
      .join("\n");
    expect(notTheRemedy).not.toContain("move what is in the way");
  });
});

describe("an installation that ships nothing", () => {
  /**
   * The listing is not a failure — exit 0 stands — but it carries the same
   * remedy as the throw beside it, because a reader who meets one and then the
   * other has no reason to be told what to do in only one of them.
   */
  it("says so with a remedy, and still exits 0", () => {
    const r = oxyc(home(), ["list"], emptySkills());
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/ships no skills/);
    expect(r.stderr).toMatch(/\n\n {2}fix: reinstall: `curl /);
  });

  /**
   * THE OTHER HALF OF THE PAIR. The comment above claims these two carry the
   * same remedy; asserting only one of them is how a claim about a pair gets
   * made without the pair being checked. The VERDICT is what differs — nothing
   * to install is a failure, nothing to list is not.
   */
  it("fails on install, with the same remedy", () => {
    const r = oxyc(home(), ["install"], emptySkills());
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/ships no skills/);
    expect(r.stderr).toMatch(/\n\n {2}fix: reinstall: `curl /);
  });

  /**
   * COMPARED, not asserted twice — and what that adds is narrower than it
   * looks. The two cases above already match both stderrs against the same
   * literal, so REWORDING one site already reddens one of them. What only a
   * comparison sees is an APPEND: `pnpm add -g @oxy-hq/cli --force` still
   * satisfies `/fix: pnpm add -g @oxy-hq\/cli/` on both sides while the two
   * remedies have diverged. Said plainly so the next person trimming
   * duplicates knows which of the three is not redundant.
   */
  it("carries the identical remedy on both paths", () => {
    const empty = emptySkills();
    const remedyOf = (out: string) => out.match(/^ *fix: (.*)$/m)?.[1];

    // ONE scratch root: the two runs differ in the SUBCOMMAND, and a second
    // `HOME` would put an incidental difference into a comparison.
    const h = home();
    const listed = remedyOf(oxyc(h, ["list"], empty).stderr);
    const installed = remedyOf(oxyc(h, ["install"], empty).stderr);
    expect(listed).toBeDefined();
    expect(installed).toBe(listed);
  });
});
