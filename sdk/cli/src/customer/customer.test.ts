/**
 * Workspace detection, dossier paths, and the delivery-activity matcher.
 *
 * These are the ported invariants where getting it wrong is silent: a session
 * scoped to the wrong tree, a machine-specific path in a customer's identity,
 * or a record that honestly reports zero.
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { parseRemoteSlug } from "../util/git.js";
import { attributionPattern, monthsIn, renderJsonl } from "./activity.js";
import { dossierPath } from "./dossier.js";
import { attributionLine } from "./repos.js";
import { memoryDir, resolveWorkspace } from "./workspace.js";

/**
 * Every scratch directory this file makes, so `afterAll` can remove them.
 *
 * Without it each run leaves ~20 directories behind in `tmpdir()`. Harmless on
 * a CI runner that is thrown away; on a developer's machine it accumulates
 * across every `pnpm test`.
 */
const SCRATCH: string[] = [];

afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
  SCRATCH.length = 0;
});

function scratch(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-ws-"));
  SCRATCH.push(dir);
  for (const [path, content] of Object.entries(files)) {
    const full = join(dir, path);
    mkdirSync(join(full, ".."), { recursive: true });
    writeFileSync(full, content);
  }
  return dir;
}

describe("resolveWorkspace", () => {
  it("reports the repo itself when config.yml is at the root", () => {
    expect(resolveWorkspace(scratch({ "config.yml": "x" }))).toBe(".");
  });

  /** The imported-repo shape: a PROJECT that contains a workspace. */
  it("finds a workspace exactly one level down", () => {
    expect(resolveWorkspace(scratch({ "oxy/config.yml": "x", "etl/main.py": "y" }))).toBe("oxy");
  });

  /**
   * Zero is NOT ambiguous — there is simply nothing to name. A customer repo
   * may hold memory and notes and no semantic model at all, and that must
   * launch rather than refuse.
   */
  it("returns undefined for a repo with no workspace, rather than refusing", () => {
    expect(resolveWorkspace(scratch({ "README.md": "x" }))).toBeUndefined();
  });

  /**
   * ONLY AMBIGUITY REFUSES, and the asymmetry is the whole design: guessing
   * between two candidates scopes a session to the wrong tree and says nothing
   * about it, which is unrecoverable from the inside.
   */
  it("refuses two workspaces, and names both", () => {
    const dir = scratch({ "a/config.yml": "x", "b/config.yml": "y" });
    expect(() => resolveWorkspace(dir)).toThrow(/more than one Oxy workspace/);
    try {
      resolveWorkspace(dir);
    } catch (e) {
      expect((e as { detail?: string }).detail).toContain("a/config.yml");
      expect((e as { detail?: string }).detail).toContain("b/config.yml");
    }
  });

  it("does not walk into build or vcs directories looking for one", () => {
    // A stray config.yml under node_modules must not make a repo ambiguous —
    // the same class of bug as the semantic layer's duplicate-view errors.
    const dir = scratch({ "config.yml": "real", "node_modules/pkg/config.yml": "stray" });
    expect(resolveWorkspace(dir)).toBe(".");
  });

  describe(".oxyc.json override", () => {
    it("is honoured when it names a real workspace", () => {
      const dir = scratch({
        "custom/config.yml": "x",
        ".oxyc.json": JSON.stringify({ workspace: "custom" })
      });
      expect(resolveWorkspace(dir)).toBe("custom");
    });

    it("treats `oxy/` and `oxy` and `./` and `.` as the same thing", () => {
      const a = scratch({ "oxy/config.yml": "x", ".oxyc.json": '{"workspace":"oxy/"}' });
      expect(resolveWorkspace(a)).toBe("oxy");
      const b = scratch({ "config.yml": "x", ".oxyc.json": '{"workspace":"./"}' });
      expect(resolveWorkspace(b)).toBe(".");
    });

    it("reads a missing key, a null and an empty string as no override", () => {
      for (const body of ["{}", '{"workspace":null}', '{"workspace":"  "}']) {
        const dir = scratch({ "oxy/config.yml": "x", ".oxyc.json": body });
        expect(resolveWorkspace(dir)).toBe("oxy");
      }
    });

    /**
     * A repo that states its own shape and states it WRONGLY is not a repo to
     * guess about — the guess would silently disagree with what somebody wrote
     * down. Hence three outcomes, not two.
     */
    it("refuses a malformed override rather than falling back to detection", () => {
      const dir = scratch({ "oxy/config.yml": "x", ".oxyc.json": "{ not json" });
      expect(() => resolveWorkspace(dir)).toThrow(/cannot use/);
    });

    it("refuses an absolute path — that is machine-specific by definition", () => {
      const dir = scratch({
        "oxy/config.yml": "x",
        ".oxyc.json": '{"workspace":"/Users/someone"}'
      });
      expect(() => resolveWorkspace(dir)).toThrow(/cannot use/);
    });

    it("refuses a path that escapes the repo", () => {
      for (const w of ["..", "../elsewhere", "a/../.."]) {
        const dir = scratch({
          "oxy/config.yml": "x",
          ".oxyc.json": JSON.stringify({ workspace: w })
        });
        expect(() => resolveWorkspace(dir), w).toThrow(/cannot use/);
      }
    });

    it("refuses an override naming a directory with no config.yml", () => {
      const dir = scratch({ "oxy/config.yml": "x", ".oxyc.json": '{"workspace":"nowhere"}' });
      expect(() => resolveWorkspace(dir)).toThrow(/has no config.yml/);
    });
  });
});

describe("memoryDir", () => {
  /**
   * Memory is about the CUSTOMER, not about the Oxy workspace inside their
   * repo, so it stays at the repo root in BOTH layouts — never inside a
   * subdirectory workspace.
   */
  it("is at the repo root even when the workspace is a subdirectory", () => {
    expect(memoryDir("/repo")).toBe("/repo/memory");
  });
});

describe("dossierPath", () => {
  it("derives <root>/<org>/<name> from the slug", () => {
    process.env.OXYC_DOSSIER_ROOT = "/tmp/dossiers";
    expect(dossierPath("oxy-hq/acme-oxy")).toBe("/tmp/dossiers/oxy-hq/acme-oxy");
    delete process.env.OXYC_DOSSIER_ROOT;
  });

  /**
   * The slug is VALIDATED rather than pasted in: `<root>/<whatever>` would
   * happily accept an absolute path or a `..`, which is exactly the
   * machine-specific shape this scheme exists to keep out of an identity.
   */
  it("refuses anything that is not <org>/<name>", () => {
    for (const bad of ["/Users/someone/repo", "../escape/out", "just-a-name", "a/b/c", ""]) {
      expect(() => dossierPath(bad), bad).toThrow(/not a repo slug/);
    }
  });
});

describe("parseRemoteSlug", () => {
  /**
   * DISCOVERY IS BY REMOTE, NOT BY PATH. On a real machine the checkouts sit
   * under directory names matching neither the GitHub org nor, in one case,
   * the repo — a `<root>/<org>/<name>` convention finds none of them.
   */
  it("reads every remote spelling git produces", () => {
    for (const url of [
      "git@github.com:oxy-hq/oxygen-internal.git",
      "git@github.com:oxy-hq/oxygen-internal",
      "ssh://git@github.com/oxy-hq/oxygen-internal.git",
      "https://github.com/oxy-hq/oxygen-internal.git",
      "https://github.com/oxy-hq/oxygen-internal"
    ]) {
      expect(parseRemoteSlug(url), url).toBe("oxy-hq/oxygen-internal");
    }
  });

  it("returns undefined for something that is not a repo remote", () => {
    expect(parseRemoteSlug("")).toBeUndefined();
    expect(parseRemoteSlug("not-a-url")).toBeUndefined();
  });
});

describe("the attribution matcher", () => {
  const line = attributionLine("pokehouse-oxy");
  const pattern = attributionPattern("pokehouse-oxy");

  /**
   * ONE DEFINITION, TWO READERS: the briefing tells a session to write this
   * line and `oxyc activity` matches on it. A briefing that told the session a
   * different spelling would produce pull requests the reader can never find.
   */
  it("matches the exact line the briefing tells a session to write", () => {
    expect(pattern.test(`Some prose.\n${line}\nMore prose.`)).toBe(true);
  });

  it("matches at the very start and the very end of a body", () => {
    expect(pattern.test(line)).toBe(true);
    expect(pattern.test(`prose\n${line}`)).toBe(true);
  });

  /**
   * GitHub bodies are `\n` today, but the API has always been free to hand
   * back `\r\n`, and a stray CR would sit between the name and the anchor.
   */
  it("survives CRLF line endings", () => {
    expect(pattern.test(`prose\r\n${line}\r\nmore`)).toBe(true);
  });

  it("accepts the lowercase a human repairing a body by hand might write", () => {
    expect(pattern.test("customer: pokehouse-oxy\n")).toBe(true);
  });

  it("tolerates spacing nobody should have to get exactly right", () => {
    expect(pattern.test("Customer:    pokehouse-oxy   \n")).toBe(true);
  });

  it("does NOT match the name inside a sentence", () => {
    expect(pattern.test("This is for the Customer: pokehouse-oxy account, roughly.")).toBe(false);
  });

  it("does NOT match an indented list item", () => {
    expect(pattern.test("Notes:\n  - Customer: pokehouse-oxy\n")).toBe(false);
  });

  /** `x-staging` must not match when the customer is `x`. */
  it("does NOT match a longer name that starts with this one", () => {
    expect(attributionPattern("pokehouse").test("Customer: pokehouse-oxy\n")).toBe(false);
  });

  /** A repo name may contain `.`, which is a regex metacharacter. */
  it("escapes regex metacharacters in the customer name", () => {
    expect(attributionPattern("a.b").test("Customer: axb\n")).toBe(false);
    expect(attributionPattern("a.b").test("Customer: a.b\n")).toBe(true);
  });
});

describe("activity rendering", () => {
  const records = [
    {
      repo: "o/a",
      number: 1,
      title: "t",
      url: "u",
      author: "me",
      mergedAt: "2026-01-05T00:00:00Z",
      via: "own" as const,
      month: "2026-01"
    },
    {
      repo: "o/b",
      number: 2,
      title: "t",
      url: "u",
      author: "me",
      mergedAt: "2026-02-05T00:00:00Z",
      via: "shared" as const,
      month: "2026-02"
    }
  ];

  it("lists the distinct months, oldest first", () => {
    expect(monthsIn(records)).toEqual(["2026-01", "2026-02"]);
  });

  /**
   * The record is keyed by (repo, number) so a re-run is idempotent. A date
   * stamped into it would make every re-run a diff, and a file that churns
   * without changing meaning is one people stop reading.
   */
  it("is byte-identical across re-runs, with no generated-on timestamp", () => {
    const once = renderJsonl("2026-01", records);
    const twice = renderJsonl("2026-01", records);
    expect(once).toBe(twice);
    expect(once).not.toMatch(/generated|timestamp/i);
  });

  it("emits only the rows for the month asked for", () => {
    expect(renderJsonl("2026-01", records).split("\n")).toHaveLength(1);
  });
});
