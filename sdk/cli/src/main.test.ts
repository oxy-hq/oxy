/**
 * The exit-code contract, exercised through the real built binary.
 *
 * Everything else in this suite tests a function. This tests the PROGRAM,
 * because the contract an agent actually depends on is "what number came back",
 * and that is decided by argument parsing, commander's exit handling and the
 * error renderer together — none of which a unit test of any one of them
 * covers. `oxyc api user --nonsense` exiting 1 instead of 2 was exactly this
 * class, and only a test at this level would have caught it.
 *
 * `pretest` builds, so `dist/` is always current here rather than whatever was
 * last built by hand.
 */

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { ExitCode } from "./util/errors.js";

const BIN = resolve(dirname(fileURLToPath(import.meta.url)), "..", "dist", "main.mjs");

interface Run {
  status: number;
  stdout: string;
  stderr: string;
}

function oxyc(...args: string[]): Run {
  if (!existsSync(BIN)) {
    throw new Error(
      `${BIN} is missing — run \`pnpm build\` (or \`pnpm test\`, which builds first)`
    );
  }
  const r = spawnSync(process.execPath, [BIN, ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      // Point every lookup at a directory that does not exist, so nothing here
      // can read a developer's real credentials or reach a real deployment.
      OXY_CREDENTIALS_PATH: join(BIN, "..", "__no_such_credentials__.json"),
      OXYC_CACHE_DIR: join(BIN, "..", "__no_such_cache__"),
      OXY_TOKEN: "",
      NO_COLOR: "1",
      // The bare-customer cases reach `launch`, which would otherwise spawn a
      // real `claude` process. Dry-run prints the command instead — the same
      // resolution work, none of the side effect.
      OXYC_DRY_RUN: "1",
      // A dossier root under the build dir, so a launch cannot clone into a
      // developer's real `~/.oxyc`.
      OXYC_DOSSIER_ROOT: join(BIN, "..", "__no_such_dossiers__")
    }
  });
  return { status: r.status ?? -1, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

describe("exit codes", () => {
  it("succeeds on help and version", () => {
    expect(oxyc("--help").status).toBe(ExitCode.OK);
    expect(oxyc("--version").status).toBe(ExitCode.OK);
    expect(oxyc("api", "--help").status).toBe(ExitCode.OK);
    expect(oxyc("exit-codes").status).toBe(ExitCode.OK);
  });

  /**
   * `2` means "you called it wrong, stop"; `1` means "it failed, maybe retry".
   * commander's `exitOverride` does not reach subcommands, so this was 1 until
   * it was applied recursively.
   */
  it("reports a usage error as USAGE, on every command in the tree", () => {
    expect(oxyc("api", "user", "--nonsense").status).toBe(ExitCode.USAGE);
    expect(oxyc("routes", "--nonsense").status).toBe(ExitCode.USAGE);
    expect(oxyc("api").status).toBe(ExitCode.USAGE); // missing required <path>
  });

  it("reports a missing credential as AUTH, with the login command", () => {
    const r = oxyc("api", "user", "--env", "production");
    expect(r.status).toBe(ExitCode.AUTH);
    expect(r.stderr).toMatch(/not authenticated/);
    expect(r.stderr).toMatch(/oxyc login/);
  });

  it("refuses an unresolvable placeholder before making a request", () => {
    const r = oxyc("api", "{workspace}/threads", "--env", "production");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/could not resolve \{workspace\}/);
  });

  it("names an unknown placeholder as unknown, not as unresolved", () => {
    const r = oxyc("api", "{nonsense}/x", "--env", "production");
    expect(r.stderr).toMatch(/unknown placeholder/);
  });
});

describe("stream discipline", () => {
  /**
   * The rule that makes this tool pipeable: stdout carries the answer and
   * nothing else. commander writes usage errors to stdout by default, which
   * would land them in the middle of piped JSON.
   */
  it("keeps stdout empty on every failure path", () => {
    for (const args of [
      ["nosuchcommand"],
      ["api", "user", "--nonsense"],
      ["api", "user", "--env", "production"],
      ["api", "{workspace}/x", "--env", "production"]
    ]) {
      const r = oxyc(...args);
      expect(r.stdout, `stdout polluted by: oxyc ${args.join(" ")}`).toBe("");
      expect(r.stderr.length).toBeGreaterThan(0);
    }
  });

  /**
   * help-after-error is a FAILURE path — commander routes it through
   * `writeErr`, which stays on stderr so `oxyc <typo> | jq` sees nothing.
   */
  it("keeps help-after-error on stderr, unlike requested help", () => {
    const r = oxyc("api", "user", "--nonsense");
    expect(r.stdout).toBe("");
    expect(r.stderr).toMatch(/oxyc --help/);
  });
});

describe("mode mixing", () => {
  /** Each of these REPLACES the output, so combining two silently picks one. */
  it("refuses two flags that both replace the output", () => {
    expect(oxyc("api", "user", "--jq", ".", "--silent").status).toBe(ExitCode.USAGE);
    expect(oxyc("api", "user", "--silent", "--verbose").status).toBe(ExitCode.USAGE);
  });

  it("refuses --slurp without --paginate, which it has no meaning without", () => {
    const r = oxyc("api", "user", "--slurp", "--env", "production");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/--slurp requires --paginate/);
  });

  it("refuses --input together with -f, rather than silently picking one", () => {
    const r = oxyc("api", "x", "--input", "-", "-f", "a=1", "--env", "production");
    expect(r.status).toBe(ExitCode.USAGE);
  });

  /** `--md` composes with `--jq` by design: jq selects, md renders. */
  it("allows --jq with --md", () => {
    // Fails on auth, not on usage — which is the point: it got past parsing.
    expect(oxyc("api", "user", "--jq", ".", "--md", "--env", "production").status).toBe(
      ExitCode.AUTH
    );
  });
});

describe("the bare-customer form", () => {
  /**
   * `oxyc pokehouse` means `oxyc launch pokehouse`, which is what people type
   * all day. A real command must still be a command — `oxyc routes` must never
   * be read as a customer named "routes".
   */
  it("does not swallow a real command", () => {
    expect(oxyc("routes", "--help").status).toBe(ExitCode.OK);
    expect(oxyc("doctor", "--help").status).toBe(ExitCode.OK);
    expect(oxyc("rm", "--help").status).toBe(ExitCode.OK); // an alias
  });

  it("does not swallow a flag", () => {
    expect(oxyc("--version").status).toBe(ExitCode.OK);
  });

  /**
   * THE COST OF THE BARE FORM, and the thing that makes it bearable: without
   * this, `oxyc rotues` rewrites to `launch rotues` and comes back "unknown
   * customer rotues" — an error about the wrong thing entirely.
   */
  it("reports a near-miss as a mistyped COMMAND, not an unknown customer", () => {
    const r = oxyc("rotues");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/unknown command/);
    expect(r.stderr).toMatch(/did you mean `oxyc routes`/);
    // …and it still says how to reach a customer that really is called that.
    expect(r.stderr).toMatch(/oxyc launch rotues/);
  });

  it("suggests for a dropped, doubled or transposed letter", () => {
    for (const [typo, meant] of [
      ["doctro", "doctor"],
      ["schem", "schema"],
      ["openapii", "openapi"]
    ]) {
      expect(oxyc(typo as string).stderr, typo).toMatch(new RegExp(`oxyc ${meant}`));
    }
  });
});

describe("help", () => {
  /**
   * Requested help is the ANSWER, so it goes to stdout: `oxyc api --help |
   * less` and `oxyc routes --help | grep workspaces` have to work. Only
   * commander's error path is redirected to stderr.
   *
   * ASSERTED ON STDOUT, not on the exit code. An earlier version of this file
   * checked `status` alone, so re-collapsing `writeOut` onto stderr — the exact
   * bug this pins — passed the entire suite.
   */
  it("writes requested help to stdout, so it can be piped", () => {
    for (const args of [["--help"], ["api", "--help"], ["routes", "--help"]]) {
      const r = oxyc(...args);
      expect(r.status, `oxyc ${args.join(" ")}`).toBe(ExitCode.OK);
      expect(r.stdout.length, `oxyc ${args.join(" ")} wrote no help to stdout`).toBeGreaterThan(
        100
      );
      expect(r.stderr, `oxyc ${args.join(" ")} leaked help to stderr`).toBe("");
    }
    expect(oxyc("--version").stdout.trim()).toMatch(/^\d+\.\d+\.\d+$/);
  });

  /**
   * The Rust `oxy api --help` appended all ~600 routes to its epilogue, which
   * cost several thousand tokens before the first request. Discovery lives in
   * `oxyc routes <filter>` instead, and help must stay short.
   */
  it("stays short enough to read — discovery is a command, not an epilogue", () => {
    expect(oxyc("--help").stdout.split("\n").length).toBeLessThan(60);
    expect(oxyc("api", "--help").stdout.split("\n").length).toBeLessThan(90);
  });

  it("documents every exit code the contract defines", () => {
    const out = oxyc("exit-codes").stdout;
    for (const code of Object.values(ExitCode)) {
      expect(out, `exit code ${code} is undocumented`).toMatch(new RegExp(`^${code}\\s`, "m"));
    }
  });
});

describe("cache clear", () => {
  /**
   * The command somebody runs when they are not sure what is cached must not
   * be the one that throws at them. `clearAllCaches` swept the root with
   * `readdirSync` and no guard, so a machine that had never cached anything
   * got `ENOENT … scandir` and exit 1 — a regression from the fix that made
   * the command clear more than responses.
   */
  it("succeeds against a cache root that has never existed", () => {
    const absent = join(mkdtempSync(join(tmpdir(), "oxyc-nocache-")), "never-created");
    const r = spawnSync(process.execPath, [BIN, "cache", "clear"], {
      encoding: "utf8",
      env: { ...process.env, OXYC_CACHE_DIR: absent, NO_COLOR: "1" }
    });
    expect(r.status).toBe(ExitCode.OK);
    expect(r.stderr).toMatch(/nothing cached/);
  });
});

describe("guide", () => {
  /**
   * The one mechanism that reaches ANY agent in ANY harness with no install
   * step. It is only useful if it is complete enough to act on, so this pins
   * the parts an agent cannot work out on its own.
   */
  it("carries the discovery loop, the placeholders and the exit codes", () => {
    const r = oxyc("guide");
    expect(r.status).toBe(ExitCode.OK);
    for (const must of [
      "oxyc routes",
      "oxyc schema",
      "{workspace}",
      "--jq",
      "--md",
      "sql/query",
      "whoami"
    ]) {
      expect(r.stdout, `the guide never mentions ${must}`).toContain(must);
    }
  });

  /** Every documented exit code, so an agent can branch without a second call. */
  it("lists every exit code the contract defines", () => {
    const out = oxyc("guide").stdout;
    for (const code of Object.values(ExitCode)) {
      expect(out, `exit code ${code} is missing from the guide`).toMatch(
        new RegExp(`\\b${code}\\b`)
      );
    }
  });

  /** It goes on stdout, because the whole point is `oxyc guide >> AGENTS.md`. */
  it("writes to stdout so it can be redirected into a file", () => {
    const r = oxyc("guide");
    expect(r.stdout.length).toBeGreaterThan(500);
    expect(r.stderr).toBe("");
  });

  /**
   * It lands in a context window on every turn, so each line has to earn its
   * place — the detail belongs behind `routes` and `schema`.
   */
  it("stays short enough to sit in a context file", () => {
    expect(oxyc("guide").stdout.split("\n").length).toBeLessThan(70);
  });
});
