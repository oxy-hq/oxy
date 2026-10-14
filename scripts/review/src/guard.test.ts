import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { guardHook, insideCheckout, toolProblem } from "./guard.ts";
import { agentEnv, redact, secretValues } from "./redact.ts";

const repo = mkdtempSync(join(tmpdir(), "guard-"));
mkdirSync(join(repo, "src"));
writeFileSync(join(repo, "src", "a.rs"), "fn a() {}");
symlinkSync("/etc/hosts", join(repo, "src", "escape"));

test("paths must resolve inside the checkout, symlinks included", () => {
  assert.ok(insideCheckout("src/a.rs", repo));
  assert.ok(insideCheckout(join(repo, "src/a.rs"), repo));
  assert.ok(insideCheckout(undefined, repo), "no path means the checkout itself");
  assert.ok(!insideCheckout("/proc/self/environ", repo));
  assert.ok(!insideCheckout("../../etc/passwd", repo));
  assert.ok(!insideCheckout("src/escape", repo), "a link the PR adds cannot point outside");
  assert.ok(!insideCheckout({ path: "/etc/passwd" }, repo), "a path that is not a string");
});

test("Read, Grep and Glob stay in the checkout", () => {
  assert.equal(toolProblem("Read", { file_path: "src/a.rs" }, repo), null);
  assert.match(toolProblem("Read", { file_path: "/proc/1/environ" }, repo) ?? "", /outside/);
  assert.match(toolProblem("Grep", { pattern: "x", path: "/home" }, repo) ?? "", /outside/);
  assert.equal(toolProblem("Glob", { pattern: "**/*.rs" }, repo), null);
  assert.match(toolProblem("Glob", { pattern: "/proc/*/environ" }, repo) ?? "", /outside/);
  assert.match(toolProblem("Glob", { pattern: "../../**/*.env" }, repo) ?? "", /outside/);
});

test("shell commands: plain reads pass, anything that chains, expands or escapes does not", () => {
  const ok = [
    "git diff abc123 def456 -- src/a.rs",
    "git diff abc123..def456",
    "git log -L 10,20:src/a.rs",
    "git blame -L 1,5 src/a.rs",
    "ast-grep -p 'foo($A)' -l rust src",
    "ast-grep run -p 'tokio::spawn($$$)' --json=compact src",
    "ast-grep outline src/a.rs --view signatures",
    "git log -S'enforce_guard' --oneline",
    "git log --format='%h %s' HEAD~3..HEAD -- src/a.rs"
  ];
  for (const c of ok) assert.equal(toolProblem("Bash", { command: c }, repo), null, c);
  const refused = [
    "git diff --no-index /proc/self/environ /dev/null",
    "git diff HEAD --output=/tmp/x",
    "git log | head",
    "git show HEAD; cat /etc/passwd",
    "git log $(cat x)",
    'git log "$HOME"',
    "git show HEAD > out.txt",
    "git blame ../../outside.rs",
    "ast-grep -p 'x' -U src",
    "ast-grep new rule",
    "git diff '--no-index' '/proc/self/environ' /dev/null",
    "git diff --no-index '/proc/self/environ' '/dev/null'",
    "git blame --contents '/etc/hosts' src/a.rs",
    "git blame -S revs.txt src/a.rs",
    "git diff -O/proc/self/environ",
    "git log '--output=/tmp/x'",
    "ast-grep scan -c '/etc/sgconfig.yml'",
    "git show 'HEAD' '../../outside'",
    "git log 'unbalanced"
  ];
  for (const c of refused) assert.notEqual(toolProblem("Bash", { command: c }, repo), null, c);
});

test("the hook denies with a reason and lets the rest through", async () => {
  const hook = guardHook(repo);
  assert.deepEqual(await hook({ tool_name: "Read", tool_input: { file_path: "src/a.rs" } }), {});
  const denied = await hook({ tool_name: "Read", tool_input: { file_path: "/proc/self/environ" } });
  assert.equal(denied.hookSpecificOutput?.permissionDecision, "deny");
});

test("the agent keeps its model key but no other secret; posted text is scrubbed", () => {
  const env = {
    PATH: "/bin",
    ANTHROPIC_API_KEY: "sk-ant-aaaaaaaaaaaaaaaa",
    GH_TOKEN: "ghs_bbbbbbbbbbbbbbbbbb",
    GITHUB_TOKEN: "ghs_cccccccccccccc",
    ACTIONS_ID_TOKEN_REQUEST_TOKEN: "x".repeat(20),
    SLACK_BOT_TOKEN: "xoxb-dddddddddddddd",
    HOME: "/root"
  };
  assert.deepEqual(Object.keys(agentEnv(env)).sort(), ["ANTHROPIC_API_KEY", "HOME", "PATH"]);
  const secrets = secretValues(env);
  assert.ok(
    secrets.includes("sk-ant-aaaaaaaaaaaaaaaa") && secrets.includes("ghs_bbbbbbbbbbbbbbbbbb")
  );
  assert.ok(!secrets.includes("/bin"));
  assert.equal(
    redact("key sk-ant-aaaaaaaaaaaaaaaa and ghs_bbbbbbbbbbbbbbbbbb", secrets),
    "key [redacted] and [redacted]"
  );
});
