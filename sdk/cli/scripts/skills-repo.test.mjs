// The skills bundle installs everywhere, and the public repo renders from it.

import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { problems, readBundle, render, repoSkillProblems, summary } from "./skills-repo.mjs";

const SCRATCH = [];
afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
});

function scratch() {
  const dir = mkdtempSync(join(tmpdir(), "oxy-skills-repo-"));
  SCRATCH.push(dir);
  return dir;
}

/** A bundle of one skill, written verbatim. */
function bundleOf(name, skillMd, extra = {}) {
  const root = scratch();
  mkdirSync(join(root, name), { recursive: true });
  writeFileSync(join(root, name, "SKILL.md"), skillMd);
  for (const [path, body] of Object.entries(extra)) {
    mkdirSync(join(root, name, path, ".."), { recursive: true });
    writeFileSync(join(root, name, path), body);
  }
  return root;
}

const problemsOf = (root) => readBundle(root).flatMap(problems);

describe("the shipped bundle", () => {
  it("is installable by any Agent Skills reader", () => {
    const bad = readBundle().flatMap((s) => problems(s).map((p) => `${s.name}: ${p}`));
    expect(bad).toEqual([]);
  });
});

describe("problems", () => {
  it("rejects a description whose unquoted colon breaks strict YAML", () => {
    // The shape four shipped skills had: fine for Claude Code, skipped by `npx skills`.
    const root = bundleOf(
      "oxy-x",
      "---\nname: oxy-x\ndescription: Use when a run lands `failed`: retry\n---\n"
    );
    expect(problemsOf(root).join()).toMatch(/not YAML/);
  });

  it("accepts the same description as a block scalar", () => {
    const md = "---\nname: oxy-x\ndescription: >-\n  Use when a run lands `failed`: retry\n---\n";
    expect(problemsOf(bundleOf("oxy-x", md))).toEqual([]);
  });

  it("rejects a name that is not its directory", () => {
    const root = bundleOf("oxy-x", "---\nname: oxy-y\ndescription: d\n---\n");
    expect(problemsOf(root).join()).toMatch(/does not match its directory/);
  });

  it("rejects a description over the spec's 1024 characters", () => {
    const root = bundleOf("oxy-x", `---\nname: oxy-x\ndescription: ${"a".repeat(1025)}\n---\n`);
    expect(problemsOf(root).join()).toMatch(/1024/);
  });

  it("rejects links that escape the skill or name a missing file", () => {
    const body = "[out](../../docs/x.md) [gone](references/gone.md) [ok](references/ok.md)";
    const root = bundleOf("oxy-x", `---\nname: oxy-x\ndescription: d\n---\n${body}\n`, {
      "references/ok.md": "ok"
    });
    const found = problemsOf(root);
    expect(found).toHaveLength(2);
    expect(found.join()).toMatch(/outside the skill: \.\.\/\.\.\/docs/);
    expect(found.join()).toMatch(/does not exist: references\/gone\.md/);
  });

  it("rejects a pointer into the private repo, in any of the skill's files", () => {
    const root = bundleOf(
      "oxy-x",
      "---\nname: oxy-x\ndescription: d\n---\n[r](references/r.md)\n",
      {
        "references/r.md": "See `crates/core/src/config/model.rs:2435`."
      }
    );
    expect(problemsOf(root).join()).toMatch(/references\/r\.md points into the private repo/);
  });

  it("checks templates for private pointers too, since render publishes them", () => {
    const root = bundleOf("oxy-x", "---\nname: oxy-x\ndescription: d\n---\n", {
      "templates/view.yml": "# see internal-docs/semantic.md\nname: v\n"
    });
    expect(problemsOf(root).join()).toMatch(/templates\/view\.yml points into the private repo/);
  });

  it("ignores URLs and in-page anchors", () => {
    const body = "[a](https://oxy.tech) [b](#publishing) [c](mailto:support@oxy.tech)";
    expect(
      problemsOf(bundleOf("oxy-x", `---\nname: oxy-x\ndescription: d\n---\n${body}\n`))
    ).toEqual([]);
  });
});

describe("render", () => {
  it("replaces the managed paths and leaves the rest of the repo alone", () => {
    const repo = scratch();
    mkdirSync(join(repo, "skills", "oxy-retired"), { recursive: true });
    writeFileSync(join(repo, "skills", "oxy-retired", "SKILL.md"), "old");
    writeFileSync(join(repo, "LICENSE"), "MIT");

    const skills = render(repo, { sourceSha: "0123456789abcdef" });

    expect(existsSync(join(repo, "skills", "oxy-retired"))).toBe(false);
    expect(readFileSync(join(repo, "LICENSE"), "utf8")).toBe("MIT");
    for (const s of skills) expect(existsSync(join(repo, "skills", s.name, "SKILL.md"))).toBe(true);

    const readme = readFileSync(join(repo, "README.md"), "utf8");
    for (const s of skills) expect(readme).toContain(`| \`${s.name}\` |`);
    expect(readme).toContain("0123456789ab");
  });

  it("writes a marketplace whose plugin is versioned by commit and keeps the old name working", () => {
    const repo = scratch();
    render(repo);
    const read = (f) => JSON.parse(readFileSync(join(repo, ".claude-plugin", f), "utf8"));
    const market = read("marketplace.json");
    const plugin = read("plugin.json");

    expect(market.plugins).toEqual([expect.objectContaining({ name: "oxy", source: "./" })]);
    expect(market.renames).toEqual({ "oxy-skills": "oxy" });
    expect(plugin.name).toBe("oxy");
    // A pinned version holds every install on its cached copy until someone bumps it.
    expect(plugin).not.toHaveProperty("version");
    expect(market.plugins[0]).not.toHaveProperty("version");
  });
});

describe("summary", () => {
  it("takes the first sentence and flattens a folded description", () => {
    expect(summary("Use when X.\n  Also Y. And Z.")).toBe("Use when X.");
    expect(summary("No full stop here")).toBe("No full stop here");
  });

  it("caps a run-on first sentence on a word boundary", () => {
    const line = summary(`${"word ".repeat(80)}.`);
    expect(line.length).toBeLessThanOrEqual(220);
    expect(line).toMatch(/word…$/);
  });

  it("does not end a sentence at an abbreviation", () => {
    expect(summary("Use for rollups, e.g. hourly sales. More.")).toBe(
      "Use for rollups, e.g. hourly sales."
    );
  });

  it("escapes angle brackets so a table cell is not read as HTML", () => {
    expect(summary("Served at /customer-apps/<org>/<slug>/.")).toBe(
      "Served at /customer-apps/&lt;org&gt;/&lt;slug&gt;/."
    );
  });
});

describe("repoSkillProblems", () => {
  it("names every .claude/skills entry that would not load, wherever it is", () => {
    const root = scratch();
    const skill = (dir, name, md) => {
      mkdirSync(join(root, dir, ".claude", "skills", name), { recursive: true });
      writeFileSync(join(root, dir, ".claude", "skills", name, "SKILL.md"), md);
    };
    skill("a", "fine", "---\nname: fine\ndescription: >-\n  ok: yes\n---\n");
    skill("b", "colon", "---\nname: colon\ndescription: breaks: strict YAML\n---\n");
    skill("c", "renamed", "---\nname: other\ndescription: d\n---\n");
    // A platform skill linked into a crate, after the skill was renamed.
    mkdirSync(join(root, "d", ".claude", "skills"), { recursive: true });
    symlinkSync("../../../gone", join(root, "d", ".claude", "skills", "linked"));
    execFileSync("git", ["init", "-q"], { cwd: root });
    execFileSync("git", ["add", "-A"], { cwd: root });

    const found = repoSkillProblems(root);
    expect(found).toHaveLength(3);
    expect(found.join("\n")).toMatch(/b\/\.claude\/skills\/colon: frontmatter is not YAML/);
    expect(found.join("\n")).toMatch(/c\/\.claude\/skills\/renamed: name "other"/);
    expect(found.join("\n")).toMatch(
      /d\/\.claude\/skills\/linked: a symlink to \.\.\/\.\.\/\.\.\/gone/
    );
  });
});
