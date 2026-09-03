/**
 * `oxyc validate`, against the schemas the Rust types generate.
 *
 * The schema mapping is the part worth pinning: a file kind mapped to the
 * wrong schema validates cleanly against rules that do not apply to it, which
 * is worse than not validating at all.
 */

import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";
import { ExitCode } from "../util/errors.js";
import {
  formatsInSchemaPosition,
  listValidatable,
  schemaFor,
  walkable,
  whyUnchecked,
  whyUnreadable
} from "./validate.js";

const SCRATCH: string[] = [];
afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
});

function workspace(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-validate-"));
  SCRATCH.push(dir);
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), content);
  }
  return dir;
}

const BIN = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "dist", "main.mjs");
const SCHEMAS = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "json-schemas");

/**
 * Run the real binary, so the EXIT CODE is what is under test.
 *
 * `runValidate` throws a `CliError`; which number that becomes is decided by
 * the renderer in `main.ts`, and an agent branches on the number. Nothing but
 * the program answers that.
 */
function oxycValidate(cwd: string, args: string[], schemasDir = SCHEMAS, home?: string) {
  if (!existsSync(BIN)) throw new Error(`${BIN} missing — run \`pnpm build\``);
  const r = spawnSync(process.execPath, [BIN, "validate", ...args], {
    cwd,
    encoding: "utf8",
    env: {
      ...process.env,
      // `os.homedir()` reads $HOME on POSIX, so the home ceiling is reachable
      // from a test without going anywhere near the developer's real one.
      ...(home ? { HOME: home } : {}),
      OXYC_SCHEMAS_DIR: schemasDir,
      OXY_CREDENTIALS_PATH: join(BIN, "..", "__no_creds__.json"),
      OXYC_CACHE_DIR: join(BIN, "..", "__no_cache__"),
      OXY_TOKEN: "",
      NO_COLOR: "1"
    }
  });
  return { status: r.status ?? -1, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

/**
 * A schemas directory holding only the named files.
 *
 * This is how the `unchecked` path is reached without breaking the install: a
 * workspace file whose schema is absent here was NOT checked, and the whole
 * point of the accounting is that it must not be counted as valid.
 */
function partialSchemas(keep: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-schemas-"));
  SCRATCH.push(dir);
  for (const name of keep) copyFileSync(join(SCHEMAS, name), join(dir, name));
  return dir;
}

/**
 * Is `mkfifo` available? Probed ONCE, at load.
 *
 * There is no Node API for a FIFO, and two cases below need one. Returning
 * early from the test body was the first attempt and it was wrong in the way
 * this branch has now caught twice: a body that returns is reported GREEN, so
 * a box without `mkfifo` showed two vacuous passes and a warning nobody
 * reads. `it.skipIf` marks them skipped, which is what the comment claimed.
 * The runner this must pass on is arm64 Linux; skipping elsewhere loses no
 * coverage that matters.
 */
const CAN_MKFIFO = (() => {
  const probe = mkdtempSync(join(tmpdir(), "oxyc-mkfifo-"));
  SCRATCH.push(probe);
  return spawnSync("mkfifo", [join(probe, "probe")], { encoding: "utf8" }).status === 0;
})();

function mkfifo(path: string): void {
  const r = spawnSync("mkfifo", [path], { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`mkfifo failed: ${r.error?.message ?? r.stderr}`);
}

/** A workspace root — `findWorkspace` looks for `config.yml`, so it needs one. */
const MINIMAL_CONFIG = "databases: []\nmodels: []\n";

describe("schemaFor", () => {
  it("maps each file kind to its own schema", () => {
    expect(schemaFor("orders.automation.yml")).toBe("workflow.json");
    expect(schemaFor("analyst.agentic.yml")).toBe("agentic.json");
    expect(schemaFor("sales.app.yml")).toBe("app.json");
    expect(schemaFor("config.yml")).toBe("config.json");
  });

  /**
   * `.procedure.yml` and `.workflow.yml` are the retired spellings of
   * `.automation.yml`; the platform still accepts both, so a validator that
   * skipped them would report a workspace as clean without checking them.
   */
  it("accepts the retired automation spellings", () => {
    expect(schemaFor("x.procedure.yml")).toBe("workflow.json");
    expect(schemaFor("x.workflow.yml")).toBe("workflow.json");
  });

  it("ignores YAML it has no schema for", () => {
    expect(schemaFor("docker-compose.yml")).toBeUndefined();
    expect(schemaFor("semantics/views/orders.view.yml")).toBeUndefined();
  });

  /** `config.yml` is matched as a whole name, in any directory. */
  it("finds config.yml in a subdirectory", () => {
    expect(schemaFor("oxy/config.yml")).toBe("config.json");
  });

  /** …but not a file that merely ends in those letters. */
  it("does not match a lookalike name", () => {
    expect(schemaFor("myconfig.yml")).toBeUndefined();
    expect(schemaFor("notanapp.yml")).toBeUndefined();
  });
});

describe("the walk", () => {
  it("finds every validatable file and nothing else", () => {
    const dir = workspace({
      "config.yml": "x: 1",
      "flows/orders.automation.yml": "x: 1",
      "apps/sales.app.yml": "x: 1",
      "semantics/views/orders.view.yml": "x: 1",
      "README.md": "x"
    });
    expect(listValidatable(dir)).toEqual([
      "apps/sales.app.yml",
      "config.yml",
      "flows/orders.automation.yml"
    ]);
  });

  /**
   * A stray copy under a build directory is the same class of bug as the
   * semantic layer's "duplicate view name" errors — it reports problems about
   * a file nobody edits.
   */
  it("does not descend into build or vcs directories", () => {
    for (const skip of ["node_modules", "target", ".git", "dist", ".worktrees"]) {
      expect(walkable(skip), skip).toBe(false);
    }
    const dir = workspace({
      "config.yml": "x: 1",
      "node_modules/pkg/config.yml": "junk: true"
    });
    expect(listValidatable(dir)).toEqual(["config.yml"]);
  });
});

describe("the .yaml spelling", () => {
  /**
   * The first version mapped only `.yml`. A workspace authored in `.yaml` was
   * walked, matched nothing, and reported clean without a file being read —
   * the worst available answer, since it is the one a caller acts on. Monaco
   * validates both spellings, so the product already accepted them.
   */
  it("maps every kind under both spellings", () => {
    for (const [yml, yaml] of [
      ["x.automation.yml", "x.automation.yaml"],
      ["x.agentic.yml", "x.agentic.yaml"],
      ["x.app.yml", "x.app.yaml"],
      ["x.agent.test.yml", "x.agent.test.yaml"],
      ["config.yml", "config.yaml"]
    ]) {
      expect(schemaFor(yaml as string), yaml).toBe(schemaFor(yml as string));
      expect(schemaFor(yaml as string), yaml).toBeDefined();
    }
  });

  it("walks a .yaml file as readily as a .yml one", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "a.app.yaml": "x",
      "b.app.yml": "x"
    });
    expect(listValidatable(root)).toEqual(["a.app.yaml", "b.app.yml", "config.yml"]);
  });
});

describe("the verdict", () => {
  it("reports a clean workspace as valid, and exits 0", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const r = oxycValidate(root, []);
    expect(r.status).toBe(0);
    expect(r.stdout).toMatch(/1 file\(s\) valid/);
  });

  /**
   * FINDINGS ARE A FAILURE, NOT A REQUEST ERROR. `REQUEST` (6) means the
   * deployment refused the call; nothing here talks to a deployment, and an
   * agent branching on 6 would go looking at the network for a YAML problem.
   */
  it("exits FAILURE on a violation, not REQUEST", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "broken.app.yml": "tasks: not-a-list\n"
    });
    const r = oxycValidate(root, []);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.status).not.toBe(ExitCode.REQUEST);
    expect(r.stdout + r.stderr).toMatch(/broken\.app\.yml/);
  });

  it("reports a YAML syntax error as one finding, not as a crash", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "bad.app.yml": "tasks: [\n  unclosed\n"
    });
    const r = oxycValidate(root, []);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stdout + r.stderr).toMatch(/bad\.app\.yml/);
  });
});

describe("--file", () => {
  it("checks one file and leaves the rest of the workspace alone", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      // `app.json` requires both `display` and `tasks`.
      "ok.app.yml": "display: []\ntasks: []\n",
      "broken.app.yml": "tasks: not-a-list\n"
    });
    const r = oxycValidate(root, ["--file", "ok.app.yml"]);
    expect(r.status).toBe(0);
    expect(r.stdout + r.stderr).not.toMatch(/broken\.app\.yml/);
  });

  /**
   * A SYMLINK KEEPS ITS OWN NAME. `schemaFor` decides a file's kind from its
   * basename, so resolving the argument through the link changed what the file
   * IS — `ln -s tpl/base.yml my.app.yml` became `base.yml`, matched nothing,
   * and `--file` rejected a file the whole-workspace walk validates happily.
   * Both directions are asserted here, because the bug was the DISAGREEMENT:
   * either answer alone looks reasonable.
   *
   * ONLY THE FIRST HALF IS A GUARD. Restoring `physical()` around the argument
   * turns `--file` red on both its assertions; making the walk disagree would
   * take resolving names inside it, which no one-token change does. The `all`
   * half is here to state what the right answer is, not to catch a regression —
   * worth knowing before anyone trims it as redundant.
   */
  it("decides a symlinked file's kind the same way the walk does", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "tpl/base.yml": "display: []\ntasks: []\n"
    });
    symlinkSync(join(root, "tpl", "base.yml"), join(root, "my.app.yml"), "file");

    const one = oxycValidate(root, ["--file", "my.app.yml"]);
    expect(one.status, one.stderr).toBe(0);
    expect(one.stderr).not.toMatch(/does not know how to check/);

    // `tpl/base.yml` is not a validatable kind, so the walk checks config.yml
    // and the link — under the link's name, via `Dirent`, which does not follow.
    const all = oxycValidate(root, []);
    expect(all.status, all.stderr).toBe(0);
    expect(all.stdout).toMatch(/2 file\(s\) valid/);
  });

  /**
   * A typo'd path is NOT_FOUND (5), never a parse error. Reading it outside
   * the YAML try is what buys that: reported as a parse failure it said
   * "ENOENT: no such file or directory", which sends the reader to look
   * inside a file for a problem that is its absence.
   */
  it("says NOT_FOUND for a path that does not exist", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const r = oxycValidate(root, ["--file", "typo.app.yml"]);
    expect(r.status).toBe(ExitCode.NOT_FOUND);
    expect(r.stderr).toMatch(/no such file/);
    expect(r.stderr).not.toMatch(/ENOENT/);
  });

  /** A kind with no schema is a USAGE error that lists what it does check. */
  it("says USAGE, and what it does check, for an unknown kind", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG, "notes.yml": "a: 1\n" });
    const r = oxycValidate(root, ["--file", "notes.yml"]);
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/\.app\.yml/);
  });
});

describe("a filesystem condition is never a parse error", () => {
  /**
   * THE RULE, STATED ONCE FOR BOTH BRANCHES. The walk skips a directory link
   * because `apps.app.yml` naming a directory reaches `readFileSync` and comes
   * back `EISDIR`, which the YAML try then reports as a `(parse)` finding —
   * sending the reader to look INSIDE a file for a problem that is the file
   * being a directory. `--file` reached the same `readFileSync`, because
   * `existsSync` is true for a directory, and kept the exception.
   */
  it("refuses --file naming a real directory, rather than reporting EISDIR", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    mkdirSync(join(root, "plain.app.yml"));

    const r = oxycValidate(root, ["--file", "plain.app.yml"]);
    // FAILURE, not NOT_FOUND: a directory named `x.app.yml` is emphatically
    // there, and 5 tells a caller branching on the number that it is not.
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/not a file/);
    expect(r.stderr).toMatch(/it is a directory/);
    expect(r.stdout + r.stderr).not.toMatch(/EISDIR/);
    expect(r.stdout + r.stderr).not.toMatch(/\(parse\)/);
  });

  it("refuses --file naming a link to a directory the same way", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const target = workspace({ "unrelated.md": "a real directory\n" });
    symlinkSync(target, join(root, "apps.app.yml"), "dir");

    const r = oxycValidate(root, ["--file", "apps.app.yml"]);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stdout + r.stderr).not.toMatch(/EISDIR/);
  });
});

describe("why a link could not be followed", () => {
  /**
   * THE REASON IS CHECKED, NOT ASSUMED. Every `stat` failure used to print
   * "the target is gone", which is true for ENOENT and a claim about a
   * condition nobody established for the rest. A self-referential link throws
   * ELOOP — worth naming on a walk that deliberately avoids needing cycle
   * detection, since "the target is gone" sends you looking for a target.
   */
  it("names a link cycle as a cycle, not as a missing target", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync("loop.app.yml", join(root, "loop.app.yml"), "file");

    const r = oxycValidate(root, []);
    expect(r.status, r.stderr).toBe(0);
    // One shared reason goes on the headline, so path and reason are on
    // separate lines — asserted independently, not as one concatenation.
    expect(r.stderr).toMatch(/the link points at itself, or round a cycle/);
    expect(r.stderr).toMatch(/loop\.app\.yml/);
    expect(r.stderr).not.toMatch(/the target is gone/);
  });

  it("carries the code into --json so a caller can branch on it", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync("loop.app.yml", join(root, "loop.app.yml"), "file");

    const r = oxycValidate(root, ["--json"]);
    expect(JSON.parse(r.stdout).broken).toEqual([{ path: "loop.app.yml", code: "ELOOP" }]);
  });
});

describe("both branches answer one condition the same way", () => {
  /**
   * `--file` threw `no such file` for EVERY stat failure, so a self-referential
   * link — which `ls` shows you — was reported as not existing, while the walk
   * branch named it a cycle. Two contradictory answers to one condition, and
   * the `--file` one was the inaccurate half.
   */
  it("names a link cycle as a cycle under --file, not as a missing file", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync("loop.app.yml", join(root, "loop.app.yml"), "file");

    const r = oxycValidate(root, ["--file", "loop.app.yml"]);
    // The code has to agree with the message. 5 means the path does not exist
    // — returning it here tells an agent the opposite of what it just read.
    // FAILURE, not NOT_FOUND — 5 would tell a caller branching on the number
    // the opposite of what the message beside it says. (`toBe` already
    // forecloses 5; a second `not.toBe` would read as a guard without being
    // one, which this branch has removed several of.)
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/cannot read loop\.app\.yml/);
    expect(r.stderr).toMatch(/points at itself, or round a cycle/);
    expect(r.stderr).not.toMatch(/no such file/);
  });

  /** A genuinely absent path keeps the message that fits it. */
  it("still says no such file for a path that is actually absent", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const r = oxycValidate(root, ["--file", "typo.app.yml"]);
    expect(r.status).toBe(ExitCode.NOT_FOUND);
    expect(r.stderr).toMatch(/no such file/);
  });

  /**
   * THE LAST DOOR TO THE WHOLE-RUN ABORT. A FIFO named `x.app.yml` is not a
   * `Dirent` directory, has a `.yml` name, matches `schemaFor`, and is not a
   * symlink — so it cleared every arm, reached `validateFile`, and took its
   * `not a file` throw and every other file with it. `walk` now emits regular
   * files and links to them, full stop.
   */
  it.skipIf(!CAN_MKFIFO)("does not let a FIFO named .app.yml abort the walk", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "good.app.yml": "display: []\ntasks: []\n"
    });
    mkfifo(join(root, "pipe.app.yml"));

    const r = oxycValidate(root, []);
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toMatch(/2 file\(s\) valid/);
    expect(r.stderr).toMatch(/not a regular file/);
    expect(r.stderr).toMatch(/pipe\.app\.yml/);
  });

  /**
   * A link to a FIFO clears the directory arm the same way a link to a regular
   * file does, so the stat has to ask `isFile()` and not merely
   * `!isDirectory()` — otherwise the link is emitted and `validateFile`'s
   * `not a file` throw aborts the run through the symlink door instead.
   */
  it.skipIf(!CAN_MKFIFO)("does not let a link to a FIFO abort the walk either", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "good.app.yml": "display: []\ntasks: []\n"
    });
    const pipes = workspace({});
    mkfifo(join(pipes, "pipe"));
    symlinkSync(join(pipes, "pipe"), join(root, "linked.app.yml"), "file");

    const r = oxycValidate(root, []);
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toMatch(/2 file\(s\) valid/);
    expect(r.stderr).toMatch(/not a regular file/);
    expect(r.stderr).toMatch(/linked\.app\.yml/);
  });

  /** And an errno the reader should not have to look up gets a sentence. */
  it("explains ENOTDIR rather than printing the code", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync("config.yml/nested", join(root, "x.app.yml"), "file");

    const r = oxycValidate(root, []);
    // Collected, not emitted — so the run continues, which is the whole point.
    expect(r.status, r.stderr).toBe(0);
    expect(r.stderr).toMatch(/a component of the path is a file/);
    expect(r.stderr).toMatch(/x\.app\.yml/);
    expect(r.stderr).not.toMatch(/— ENOTDIR/);
  });
});

describe("--json always emits a document", () => {
  /**
   * An empty stdout makes `oxyc validate --json | jq` fail on the one workspace
   * shape whose answer is simply "nothing to check" — the early return sat
   * above the `--json` block, so the promise of a document had an exception
   * exactly where a script is least able to handle one.
   */
  it("emits an empty result rather than nothing, for a workspace with no YAML", () => {
    const root = workspace({ "notes.md": "nothing here\n" });
    const r = oxycValidate(root, ["--json"]);
    expect(r.status).toBe(0);
    expect(JSON.parse(r.stdout)).toEqual({
      checked: 0,
      unchecked: [],
      broken: [],
      findings: []
    });
  });
});

describe("files it could not check", () => {
  /**
   * THE HEADLINE FIX. `validateFile` returns `[]` for "checked, no findings"
   * and `undefined` for "no schema for this in this installation" — collapsing
   * the two printed "N file(s) valid" over files nothing had read.
   */
  it("names them instead of counting them as valid", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "dash.app.yml": "tasks: []\n"
    });
    // `app.json` withheld: `dash.app.yml` maps to a schema this install lacks.
    const r = oxycValidate(root, [], partialSchemas(["config.json"]));
    expect(r.stderr).toMatch(/1 file\(s\) NOT checked/);
    expect(r.stderr).toMatch(/dash\.app\.yml/);
    // And the verdict counts only what was actually read.
    expect(r.stdout).toMatch(/1 file\(s\) valid/);
    expect(r.stdout).not.toMatch(/2 file\(s\) valid/);
  });

  it("reports the same split in --json", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "dash.app.yml": "tasks: []\n"
    });
    const r = oxycValidate(root, ["--json"], partialSchemas(["config.json"]));
    expect(JSON.parse(r.stdout)).toEqual({
      checked: 1,
      unchecked: [{ path: "dash.app.yml", code: "SCHEMA_MISSING" }],
      broken: [],
      findings: []
    });
  });
});

describe("symlinked directories in the walk", () => {
  /**
   * THE ARM THAT HAD NO TEST. `Dirent` does not follow links, so a link to a
   * directory answers `false` to `isDirectory()` and falls through to the
   * extension check exactly as a linked FILE does. Named `apps.app.yml` it
   * passed that check, matched `.app.yml`, and reached `readFileSync` — which
   * reported `EISDIR: illegal operation on a directory` as a YAML PARSE
   * finding. Same wrong answer the `NOT_FOUND` handling exists to prevent, in
   * through the other door.
   */
  it("skips a directory link even when its name says .app.yml", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const target = workspace({ "unrelated.md": "a real directory\n" });
    symlinkSync(target, join(root, "apps.app.yml"), "dir");

    const r = oxycValidate(root, ["--json"]);
    expect(r.status, r.stderr).toBe(0);
    expect(JSON.parse(r.stdout)).toEqual({
      checked: 1,
      unchecked: [],
      broken: [],
      findings: []
    });
    expect(r.stdout).not.toMatch(/EISDIR/);
  });

  /**
   * A link INSIDE the root costs nothing: the walk reaches the target by its
   * real path, so the file is checked — just under the name it really has.
   * This is the half that makes "silently invisible" too strong a description.
   *
   * NOT A GUARD. The link is named `apps`, so it never had an extension to
   * clear — before the fix it was skipped at `extname`, after it at the stat,
   * and the counts agree either way. This states the intent; the `.app.yml`
   * case above is what a mutation kills.
   */
  it("still checks a link's target when the target is inside the root", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "shared/x.app.yml": "display: []\ntasks: []\n"
    });
    symlinkSync(join(root, "shared"), join(root, "apps"), "dir");

    const r = oxycValidate(root, ["--json"]);
    expect(r.status, r.stderr).toBe(0);
    // config.yml + shared/x.app.yml — once, not twice, and not zero times.
    expect(JSON.parse(r.stdout).checked).toBe(2);
  });

  /**
   * A link pointing OUTSIDE the root is the one shape genuinely missed, and
   * `--file` is the escape hatch for it. Both halves asserted, because the
   * claim being pinned is the DIFFERENCE between the two branches — which is
   * written down nowhere else. Like the case above this is a statement rather
   * than a guard: the link has no extension, so both versions skip it.
   */
  it("does not walk a link out of the root, though --file reads through it", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const outside = workspace({ "x.app.yml": "display: []\ntasks: []\n" });
    symlinkSync(outside, join(root, "apps"), "dir");

    const walked = oxycValidate(root, ["--json"]);
    expect(JSON.parse(walked.stdout).checked).toBe(1);

    const direct = oxycValidate(root, ["--file", "apps/x.app.yml"]);
    expect(direct.status, direct.stderr).toBe(0);
    expect(direct.stdout).toMatch(/1 file\(s\) valid/);
  });
});

describe("a broken symlink in the walk", () => {
  /**
   * ONE STALE LINK USED TO ABORT THE RUN. `walk` emitted it, `validateFile`'s
   * `existsSync` follows links and answered false, and the `NOT_FOUND` throw —
   * written for `--file`, where "no such file" answers a typo'd argument —
   * stopped the whole workspace at exit 5 with nothing said about any other
   * file. A `models/x.view.yml` pointing into a removed worktree was enough.
   */
  it("does not stop the run, and still checks every other file", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "good.app.yml": "display: []\ntasks: []\n"
    });
    symlinkSync(join(root, "gone", "target.yml"), join(root, "stale.app.yml"), "file");

    const r = oxycValidate(root, []);
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toMatch(/2 file\(s\) valid/);
  });

  /** And it is NAMED — a file nothing read must never pass as valid. */
  it("names the link rather than passing over it", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync(join(root, "gone", "target.yml"), join(root, "stale.app.yml"), "file");

    const r = oxycValidate(root, []);
    // One entry, so the reason REPLACES the category descriptor rather than
    // chaining after it — `… — could not read them — the target is gone` was
    // two clauses saying one thing.
    // The headline carries the reason (one entry, so it hoists); the path is
    // listed under it. No third assertion — `/the target is gone/` alone
    // cannot fail unless the headline match above already has.
    expect(r.stderr).toMatch(/1 file\(s\) NOT checked — the target is gone/);
    expect(r.stderr).toMatch(/stale\.app\.yml/);
    // NO REMEDY ON THIS BUCKET, so no separator either. `listSkipped` spreads a
    // possibly-empty array into `log.remedy`, which makes that function's
    // emptiness guard reachable — without it this run ends on two stray blank
    // lines, and nothing else here would notice.
    expect(r.stderr).not.toMatch(/\n\n/);
  });

  it("reports it in --json, apart from unchecked — the fix differs", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync(join(root, "gone", "target.yml"), join(root, "stale.app.yml"), "file");

    const r = oxycValidate(root, ["--json"]);
    expect(JSON.parse(r.stdout)).toEqual({
      checked: 1,
      unchecked: [],
      broken: [{ path: "stale.app.yml", code: "ENOENT" }],
      findings: []
    });
  });

  /**
   * `--file` KEEPS THE THROW — the abort was only ever wrong on the branch that
   * enumerated the file itself. But a DANGLING LINK is not an absent path:
   * `statSync` throws `ENOENT` for both, and answering "no such file" for a
   * link `ls` shows you is the same wrong answer the walk arm stopped giving.
   * `lstat` separates them, so this one reads like the walk's.
   */
  it("names a stale link as a stale link under --file, and does not claim it is absent", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync(join(root, "gone", "target.yml"), join(root, "stale.app.yml"), "file");

    const r = oxycValidate(root, ["--file", "stale.app.yml"]);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/cannot read stale\.app\.yml/);
    expect(r.stderr).toMatch(/the target is gone/);
    expect(r.stderr).not.toMatch(/no such file/);

    // HALF THE RENDER ORDER IS OBSERVABLE, AND ONLY HALF. `reportAndExit`
    // prints detail, then hint, then remedy. This error is the one run in the
    // suite carrying TWO of the three — `detail: "the target is gone"` and
    // `hint: WHOLE_WORKSPACE_HINT` — so it pins `detail → hint`.
    //
    // A first attempt pinned this from `--file typo.app.yml`, which carries a
    // HINT ALONE: it asserted that a hint prints below the error line, which
    // was never in question, and swapping the hint and remedy blocks left it
    // green. `hint → remedy` still has no producer — no `CliError` in the tool
    // carries both — so that adjacency is unpinned, and saying so beats
    // pointing at a run that cannot show it.
    // No `> -1` guard: `toMatch(/the target is gone/)` above already has it, and
    // a missing HINT makes the right-hand side `-1`, which `toBeLessThan` fails
    // on its own. An assertion foreclosed by one above it reads as a guard
    // without being one — the third this branch has removed for that.
    expect(r.stderr.indexOf("the target is gone")).toBeLessThan(
      r.stderr.indexOf("to check the whole workspace")
    );
  });

  /** A path that is genuinely absent still gets NOT_FOUND, which is 5's job. */
  it("keeps NOT_FOUND for an argument that names nothing at all", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const r = oxycValidate(root, ["--file", "typo.app.yml"]);
    expect(r.status).toBe(ExitCode.NOT_FOUND);
    expect(r.stderr).toMatch(/no such file/);
  });
});

describe("an error's remedy", () => {
  /**
   * THE ERROR PATH'S HALF OF THE SPLIT. `CliError.hint` renders through
   * `log.hint`, so a remedy handed to it printed as `→ …` — an instruction in
   * the run of elaborations, which is exactly what `log.remedy` exists to
   * prevent. `CliError.remedy` is the channel; this is its first producer, and
   * without one the wiring would have been written and never exercised.
   */
  it("prints set apart, not under the elaboration marker", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    const noSchemas = mkdtempSync(join(tmpdir(), "oxyc-noschemas-"));
    SCRATCH.push(noSchemas);

    const r = oxycValidate(root, [], noSchemas);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/no JSON Schemas are available/);
    // THE CHANNEL, NOT THE TEXT. Which sentence prints depends on `inCheckout`,
    // which resolves off `tmpdir()` — `/json-schemas` on Linux, but a `TMPDIR`
    // inside a checkout flips it to "run `pnpm build`", and an assertion on
    // "reinstall" then goes quiet with the negative one passing vacuously.
    expect(r.stderr).toMatch(/\n\n {2}fix: /);
    // DERIVED FROM WHAT PRINTED, because two previous versions of this line
    // enumerated the `inCheckout` texts and were broken by the next rewording —
    // and `/→ .*pnpm/` was only the third guess at a phrase that would hold
    // (`npm i -g` is the obvious next wording, and would break it again).
    // Reading the remedy back out of the output cannot go stale, and unlike a
    // flat `not.toMatch(/→/)` it still works once this error has a hint too.
    const printed = r.stderr.match(/fix: (.*)/)?.[1];
    expect(printed).toBeDefined();
    expect(r.stderr).not.toContain(`→ ${printed}`);
  });
});

describe("why a file was not checked", () => {
  /**
   * TWO PRODUCERS, TWO REMEDIES. The count line used to carry ONE reason for
   * both, so every skipped file was told to reinstall — right for a schema
   * absent from the installation, useless for a kind `oxyc` maps nothing to,
   * where no reinstall produces one.
   *
   * The rule now: a reason the whole bucket SHARES goes on the count, and a
   * bucket that disagrees gets one per file. This case is one file, so it
   * hoists — which makes it an instance of the shared arm, not the per-file
   * one an earlier version of this comment described.
   */
  it("tells a file with a missing schema to reinstall", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "dash.app.yml": "display: []\ntasks: []\n"
    });
    const r = oxycValidate(root, [], partialSchemas(["config.json"]));
    expect(r.stderr).toMatch(/the schema is not in this installation/);
    expect(r.stderr).toMatch(/dash\.app\.yml/);
    expect(r.stderr).toMatch(/OXYC_SCHEMAS_DIR/);
  });

  /** And the code reaches `--json`, so a caller acts on the reason. */
  it("carries the reason into --json, the way broken already did", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "dash.app.yml": "display: []\ntasks: []\n"
    });
    const r = oxycValidate(root, ["--json"], partialSchemas(["config.json"]));
    expect(JSON.parse(r.stdout).unchecked).toEqual([
      { path: "dash.app.yml", code: "SCHEMA_MISSING" }
    ]);
  });

  /**
   * ONE SHARED REASON IS PRINTED ONCE. Moving the reason per file fixed a
   * message asserting one producer's remedy for both — and then repeated a
   * long sentence once per path, which reads worse than the bug did. Counted
   * rather than matched, because "it appears" is true under both behaviours.
   */
  it("prints a reason the whole bucket shares exactly once", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync(join(root, "gone-a.yml"), join(root, "a.app.yml"), "file");
    symlinkSync(join(root, "gone-b.yml"), join(root, "b.app.yml"), "file");

    const r = oxycValidate(root, []);
    expect(r.stderr.match(/the target is gone/g) ?? []).toHaveLength(1);
    // The paths are still all listed — only the reason was hoisted.
    expect(r.stderr).toMatch(/a\.app\.yml/);
    expect(r.stderr).toMatch(/b\.app\.yml/);
  });

  /**
   * AND WHEN THEY DIFFER, EACH PATH CARRIES ITS OWN. This is the case the
   * per-file wording exists for, and the one a shared headline cannot serve.
   */
  it("puts a reason beside each path when the bucket disagrees", () => {
    const root = workspace({ "config.yml": MINIMAL_CONFIG });
    symlinkSync(join(root, "gone.yml"), join(root, "stale.app.yml"), "file");
    symlinkSync("loop.app.yml", join(root, "loop.app.yml"), "file");

    const r = oxycValidate(root, []);
    expect(r.stderr).toMatch(/stale\.app\.yml — the target is gone/);
    expect(r.stderr).toMatch(/loop\.app\.yml — the link points at itself/);
    // …and the headline stays a bare count, with no reason hoisted onto it.
    expect(r.stderr).toMatch(/2 file\(s\) NOT checked — could not read them\n/);
  });

  /**
   * `KIND_UNKNOWN` CANNOT BE PRODUCED BY A RUN — `walk` filters on
   * `!schemaFor(rel)` before emitting and `--file` throws USAGE on the same
   * predicate — so its message is pinned here directly. Without this, a
   * mutation swapping the two codes passes the whole suite, and the remedy
   * that ships the day a filter moves is the wrong one.
   */
  it("does not tell an unmapped kind to reinstall", () => {
    expect(whyUnchecked("SCHEMA_MISSING")).toEqual({
      reason: "the schema is not in this installation",
      remedy: "reinstall, or set OXYC_SCHEMAS_DIR at a checkout's json-schemas/"
    });

    // NO REMEDY AT ALL, which is the asymmetry the split exists for: no
    // reinstall produces a mapping oxyc does not have, so there is nothing to
    // hint and the field is absent rather than filled with the other one's.
    expect(whyUnchecked("KIND_UNKNOWN").reason).toMatch(/no schema for this kind/);
    expect(whyUnchecked("KIND_UNKNOWN").remedy).toBeUndefined();
  });

  /**
   * A CLEAN RUN SAYS NOTHING ABOUT SKIPS. The emptiness guard moved into
   * `listSkipped` when the call sites stopped checking, so it is now the only
   * thing standing between a spotless workspace and two `0 file(s) NOT
   * checked` warnings. Nothing asserted that until this — and the guard is
   * exactly the kind of term this branch has twice deleted for being inert,
   * which it would have looked like without a case that goes red.
   */
  it("warns about nothing when every file was checked", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "ok.app.yml": "display: []\ntasks: []\n"
    });
    const r = oxycValidate(root, []);
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toMatch(/2 file\(s\) valid/);
    expect(r.stderr).not.toMatch(/NOT checked/);
    expect(r.stderr).not.toMatch(/0 file\(s\)/);
  });

  /**
   * THE ARMS A RUN CANNOT REACH. `EACCES`/`EPERM` needs `chmod` or root and
   * `EUNKNOWN` needs a throw carrying no code at all — neither happens on CI,
   * so swapping their strings passed everything. Same gap, and same remedy, as
   * `KIND_UNKNOWN` above: assert the mapping directly.
   */
  it("explains the codes a run cannot produce", () => {
    expect(whyUnreadable("EACCES").reason).toMatch(/permission denied/);
    expect(whyUnreadable("EPERM").reason).toMatch(/permission denied/);
    expect(whyUnreadable("EUNKNOWN").reason).toMatch(/without saying why/);
    // And an errno nobody mapped falls through to itself — the fallback
    // `whyUnchecked` deliberately does NOT have, because OS codes are
    // open-ended and ours are two constants.
    expect(whyUnreadable("EMFILE")).toEqual({ reason: "EMFILE" });
  });

  /**
   * THE REMEDY IS A HINT, NOT PART OF THE REASON. Folded together, the only
   * reachable shared headline ran to 140 characters with two em dashes, the
   * second half of which was a remedy rather than a reason — the exact shape
   * the shared-reason rule had just been written to remove, surviving in the
   * one case a user actually hits.
   */
  it("puts the remedy on its own line, once, below the paths", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "a.app.yml": "display: []\ntasks: []\n",
      "b.app.yml": "display: []\ntasks: []\n"
    });
    const r = oxycValidate(root, [], partialSchemas(["config.json"]));

    expect(r.stderr).toMatch(/2 file\(s\) NOT checked — the schema is not in this installation\n/);
    // Once for two files, and not chained onto the headline.
    expect(r.stderr.match(/OXYC_SCHEMAS_DIR/g) ?? []).toHaveLength(1);
    expect(r.stderr).not.toMatch(/installation — reinstall/);
    // BELOW THE PATHS — the half the test's name promises, and the half the
    // three assertions above hold identically whether or not it is true.
    expect(r.stderr.indexOf("OXYC_SCHEMAS_DIR")).toBeGreaterThan(r.stderr.indexOf("b.app.yml"));
    // And under its own marker: `→` elaborates the line above, so a remedy
    // wearing it reads in sequence with the paths rather than apart from them.
    expect(r.stderr).toMatch(/fix: reinstall/);
    // SEPARATED ON BOTH SIDES. With a blank only above, the line attached
    // downward to whatever followed — and on the `--file` path that is a
    // `log.error` with no leading blank, so `fix:` grouped with an unrelated
    // error instead of with the warning it belongs to.
    expect(r.stderr).toMatch(/\n\n {2}fix: reinstall[^\n]*\n\n/);
  });

  /**
   * THE CAP ANNOUNCES ITSELF. `11 file(s) NOT checked` over ten listed paths
   * reads as a display bug rather than a cap.
   */
  it("says how many it elided when there are more than ten", () => {
    const files: Record<string, string> = { "config.yml": MINIMAL_CONFIG };
    for (let i = 0; i < 12; i++) files[`a${i}.app.yml`] = "display: []\ntasks: []\n";
    const r = oxycValidate(workspace(files), [], partialSchemas(["config.json"]));
    expect(r.stderr).toMatch(/12 file\(s\) NOT checked/);
    expect(r.stderr).toMatch(/… and 2 more/);
  });
});

describe("nothing read is not success", () => {
  /**
   * THE SHARP ONE. The caller named exactly one file, its schema was absent
   * from the installation, so it was never opened — and the command answered
   * exit 0. There is no partial result to defend here, and `util/errors.ts`
   * exists precisely so "printed an error and exited 0" is unrepresentable.
   * The word was fixed a round earlier; the number kept saying `valid`.
   */
  it("fails when --file named a file whose schema this installation lacks", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "dash.app.yml": "display: []\ntasks: []\n"
    });
    const r = oxycValidate(root, ["--file", "dash.app.yml"], partialSchemas(["config.json"]));
    // `status` IS THE GUARD. The two below pass under the bug as well — the
    // pre-fix stdout was `nothing checked`, which contains no "valid", and the
    // warning was already correct. They state the surrounding behaviour; the
    // number is the half that was wrong.
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/NOT checked/);
    expect(r.stdout).not.toMatch(/valid/);

    // THE GROUPING CASE THE SEPARATOR WAS REPORTED FOR: this is the run where
    // an `error:` follows the remedy immediately, and a blank only above left
    // `fix:` closer to that error than to the warning it belongs to. The
    // whole-workspace test that asserts the spacing has nothing after it, so
    // its trailing `\n\n` is satisfied by end-of-stderr.
    expect(r.stderr).toMatch(/fix: reinstall[^\n]*\n\nerror:/);
  });

  it("fails when a whole workspace was walked and nothing in it could be read", () => {
    const root = workspace({
      "a.app.yml": "display: []\ntasks: []\n",
      "b.app.yml": "display: []\ntasks: []\n"
    });
    const r = oxycValidate(root, [], partialSchemas(["config.json"]));
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/2 file\(s\) NOT checked/);
  });

  it("reports the same verdict under --json", () => {
    const root = workspace({ "a.app.yml": "display: []\ntasks: []\n" });
    const r = oxycValidate(root, ["--json"], partialSchemas(["config.json"]));
    expect(r.status).toBe(ExitCode.FAILURE);
    // The document is still emitted — the exit code is the only thing that moved.
    expect(JSON.parse(r.stdout).unchecked).toEqual([{ path: "a.app.yml", code: "SCHEMA_MISSING" }]);
  });

  /**
   * AND THE OTHER SIDE. A workspace with nothing to check is not a failure.
   *
   * What protects it is the EARLY RETURN, which fires before the verdict is
   * reached — not the `unchecked + broken > 0` conjunct, which is unreachable
   * today and which a mutation can remove without failing this. Said plainly
   * because a case that looks like it pins a term it does not is the thing
   * several rounds of this branch have been spent deleting.
   */
  it("still exits 0 for a workspace that genuinely holds nothing", () => {
    const root = workspace({ "notes.md": "nothing here\n" });
    const r = oxycValidate(root, []);
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/no validatable YAML found/);
  });

  /** A broken link is the other way in, and it counts the same. */
  it("fails when the only candidate was a link that could not be read", () => {
    const root = workspace({ "notes.md": "nothing here\n" });
    symlinkSync(join(root, "gone.yml"), join(root, "stale.app.yml"), "file");
    const r = oxycValidate(root, []);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stderr).toMatch(/1 file\(s\) NOT checked — the target is gone/);
  });
});

describe("the workspace walk", () => {
  it("finds the root from a subdirectory", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "apps/broken.app.yml": "tasks: not-a-list\n"
    });
    const r = oxycValidate(join(root, "apps"), []);
    expect(r.status).toBe(ExitCode.FAILURE);
    expect(r.stdout + r.stderr).toMatch(/apps\/broken\.app\.yml/);
  });

  /**
   * CEILINGED AT THE GIT ROOT. Unbounded, this walked to `/`, so running it
   * outside a workspace on a machine with a `~/config.yml` adopted `$HOME` as
   * the root and walked the entire home directory.
   *
   * The fixture is built so the two behaviours DISAGREE, which the obvious
   * version of this test does not: a `config.yml` sits one level ABOVE a git
   * root, and the cwd sits inside it. Ceilinged, the walk stops at the repo and
   * reports nothing validatable. Unceilinged, it climbs past the repo, adopts
   * the outer `config.yml` and reports a file valid. A fixture with no
   * `config.yml` anywhere above passes either way and proves nothing.
   */
  it("stops at the git root instead of adopting a config.yml above it", () => {
    const outer = workspace({
      "config.yml": MINIMAL_CONFIG,
      "repo/sub/notes.md": "nothing validatable here\n"
    });
    // A REAL repo: `repoRoot` shells out to `git rev-parse --show-toplevel`,
    // so a hand-made `.git/` directory is not a ceiling at all.
    const init = spawnSync("git", ["init", "-q"], { cwd: join(outer, "repo"), encoding: "utf8" });
    expect(init.status, init.stderr).toBe(0);

    const r = oxycValidate(join(outer, "repo", "sub"), []);
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/no validatable YAML found/);
    // The outer config.yml was never reached, so it was never counted.
    expect(r.stdout).not.toMatch(/file\(s\) valid/);
  });

  /**
   * THE HOME CEILING, which the first version only held when the cwd happened
   * to be under `$HOME`. Outside a repo and outside home — `/tmp`, `/srv`, a
   * container workdir — `homedir()` is never an ancestor, so the stop never
   * fired and the walk climbed to `/` exactly as before it was added. Here the
   * cwd IS under home, so the ceiling is live and the `config.yml` sitting at
   * home itself must not be adopted: a home directory is not a workspace, and
   * treating one as a root walks the whole thing.
   */
  it("does not adopt a config.yml sitting at $HOME", () => {
    const home = workspace({
      "config.yml": MINIMAL_CONFIG,
      "projects/thing/notes.md": "nothing validatable here\n"
    });
    const r = oxycValidate(join(home, "projects", "thing"), [], SCHEMAS, home);
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/no validatable YAML found/);
    expect(r.stdout).not.toMatch(/file\(s\) valid/);
  });

  /**
   * THE HOME STOP DOES WORK OF ITS OWN, separate from not adopting `~` itself:
   * without it the walk crosses home and keeps checking `/Users`, `/home`, `/`.
   * The fixture puts a `config.yml` ONE LEVEL ABOVE the home directory, so the
   * two behaviours disagree — stopped, it is never seen; unstopped, it is
   * adopted and a directory outside the user's home becomes the workspace root.
   */
  it("does not climb past $HOME into the directory above it", () => {
    const above = workspace({
      "config.yml": MINIMAL_CONFIG,
      "home/projects/notes.md": "nothing validatable here\n"
    });
    const home = join(above, "home");
    const r = oxycValidate(join(home, "projects"), [], SCHEMAS, home);
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/no validatable YAML found/);
    expect(r.stdout).not.toMatch(/file\(s\) valid/);
  });

  /**
   * THE HOME STOP ACROSS A SYMLINK, which is the shape a real machine has.
   * `os.homedir()` returns `$HOME` verbatim; `process.cwd()` is `getcwd(3)` and
   * therefore physical. On macOS `$TMPDIR` lives under `/var`, itself a link to
   * `/private/var`, so the child is handed one spelling of home and computes
   * another — `dir === home` never fires, the walk climbs past home and adopts
   * the `config.yml` above it.
   *
   * The fixture forces that shape explicitly rather than relying on the host's
   * temp directory: the other two home cases pass on a box whose `tmpdir()` is
   * already physical, which is exactly how this went unnoticed.
   */
  it("stops at $HOME even when the path to it crosses a symlink", () => {
    const scratch = workspace({
      "real/config.yml": MINIMAL_CONFIG,
      "real/home/projects/notes.md": "nothing validatable here\n"
    });
    symlinkSync(join(scratch, "real"), join(scratch, "link"), "dir");

    // HOME is the LOGICAL spelling; the cwd resolves to the physical one.
    const r = oxycValidate(
      join(scratch, "link", "home", "projects"),
      [],
      SCHEMAS,
      join(scratch, "link", "home")
    );
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/no validatable YAML found/);
    expect(r.stdout).not.toMatch(/file\(s\) valid/);
  });

  /**
   * And a real ancestor inside the tree IS still found — the stops bound the
   * walk, they do not disable it. Without this the two cases above are also
   * satisfied by a `findWorkspace` that never climbs at all.
   */
  it("still finds a config.yml in a genuine ancestor", () => {
    const root = workspace({
      "config.yml": MINIMAL_CONFIG,
      "apps/dashboards/finance/notes.md": "nothing validatable here\n"
    });
    // $HOME points elsewhere: the workspace root must be found on its own
    // merits, not because it happens to be the home directory.
    const r = oxycValidate(
      join(root, "apps", "dashboards", "finance"),
      [],
      SCHEMAS,
      workspace({ "unrelated.md": "not this tree\n" })
    );
    expect(r.status).toBe(0);
    expect(r.stdout).toMatch(/1 file\(s\) valid/);
  });

  /** With nothing above it either, the cwd is simply the answer. */
  it("falls back to the cwd outside any repo", () => {
    const root = workspace({ "notes.md": "nothing here\n" });
    const r = oxycValidate(root, []);
    expect(r.status).toBe(0);
    expect(r.stderr).toMatch(/no validatable YAML found/);
  });

  /** `build` holds rendered copies of workspace files; checking them is noise. */
  it("skips build and the vcs directories", () => {
    for (const name of ["build", "dist", "node_modules", ".git", "target", ".worktrees"]) {
      expect(walkable(name), name).toBe(false);
    }
    expect(walkable("apps")).toBe(true);
  });
});

describe("scanning a schema for formats", () => {
  /**
   * A `format` only counts beside the keywords that make its object a schema.
   * The version this replaced regex-scanned the raw JSON text, which cannot
   * tell a constraint from the word "format" appearing in data.
   */
  it("finds a format in schema position", () => {
    const seen = new Set<string>();
    formatsInSchemaPosition({ properties: { port: { type: "integer", format: "uint16" } } }, seen);
    expect([...seen]).toEqual(["uint16"]);
  });

  /**
   * THE CASE THE TEXT SCAN GOT WRONG. `default` and `examples` hold INSTANCE
   * data — a serialised config whose own key happens to be `format`.
   *
   * The data object here carries `type` as well, so it looks exactly like a
   * schema from the inside: NOT DESCENDING is the only thing that saves it,
   * which is what makes this a test of the skip rather than of the sibling
   * check downstream. A fixture whose data object is a bare `{format: "csv"}`
   * passes with the skip deleted, because the sibling check catches it anyway.
   */
  it("does not descend into default or examples, even when the data looks like a schema", () => {
    const seen = new Set<string>();
    formatsInSchemaPosition(
      {
        type: "object",
        properties: {
          export: {
            type: "object",
            default: { type: "csv", format: "wide" },
            examples: [{ type: "parquet", format: "tall" }],
            const: { type: "json", format: "nested" }
          }
        }
      },
      seen
    );
    expect([...seen]).toEqual([]);
  });

  /**
   * And the other guard, alone. An annotation keyword is not `default`, so the
   * walk DOES descend into it — only the missing schema siblings stop the
   * `format` inside from counting. Deleting the sibling check makes this fail
   * while every `default`-shaped fixture still passes.
   */
  it("does not count a format with no schema keyword beside it", () => {
    const seen = new Set<string>();
    formatsInSchemaPosition(
      {
        type: "object",
        properties: { a: { type: "string" } },
        "x-oxy-export": { format: "csv" },
        metadata: { format: "internal", owner: "platform" }
      },
      seen
    );
    expect([...seen]).toEqual([]);
  });

  it("descends through arrays and nested subschemas", () => {
    const seen = new Set<string>();
    formatsInSchemaPosition(
      {
        anyOf: [
          { type: "string", format: "date-time" },
          { items: { type: "integer", format: "int64" } }
        ]
      },
      seen
    );
    expect([...seen].sort()).toEqual(["date-time", "int64"]);
  });

  /**
   * Every format the SHIPPED schemas use must be one `registerRustFormats`
   * teaches ajv — an unknown one is silently ignored, so the constraint stops
   * applying with nothing to show for it. This is the assertion that fires
   * when a Rust type starts emitting a width nobody registered.
   */
  it("finds nothing unregistered in the schemas this package ships", () => {
    const seen = new Set<string>();
    for (const name of [
      "config.json",
      "app.json",
      "workflow.json",
      "agentic.json",
      "agent-test.json"
    ]) {
      formatsInSchemaPosition(JSON.parse(readFileSync(join(SCHEMAS, name), "utf8")), seen);
    }
    const KNOWN = new Set([
      "uint",
      "uint8",
      "uint16",
      "uint32",
      "uint64",
      "int8",
      "int16",
      "int32",
      "int64",
      "double",
      "float",
      "date",
      "time",
      "date-time",
      "duration",
      "uri",
      "uri-reference",
      "email",
      "hostname",
      "ipv4",
      "ipv6",
      "uuid",
      "regex",
      "json-pointer",
      "byte",
      "binary",
      "password",
      "int32-or-string"
    ]);
    expect([...seen].filter((f) => !KNOWN.has(f))).toEqual([]);

    // AND IT FOUND THEM. Asserting only "nothing unknown" is satisfied by a
    // walk that finds nothing at all — a sibling list too narrow to match what
    // `schemars` really emits would pass the line above and silently stop
    // checking every format in the package.
    expect(seen.size).toBeGreaterThan(0);
    for (const expected of ["uint", "uint32", "int64", "double"]) {
      expect(seen.has(expected), expected).toBe(true);
    }
  });
});
