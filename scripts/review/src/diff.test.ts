import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addedText,
  commentableLines,
  parseUnifiedDiff,
  toRanges,
  touchedOldLines
} from "./diff.ts";
import { must } from "./fixtures.ts";
import { globToRegExp, matchesAny, ownerPatternToRegExp } from "./globs.ts";

const DIFF = `diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -10,4 +10,5 @@ fn main() {
 let a = 1;
-let b = 2;
+let b = 3;
+let c = 4;

 println!();
diff --git a/web/new.ts b/web/new.ts
new file mode 100644
index 000..333
--- /dev/null
+++ b/web/new.ts
@@ -0,0 +1,2 @@
+export const x = 1;
+export const y = 2;
\\ No newline at end of file
diff --git a/old/name.md b/new/name.md
similarity index 90%
rename from old/name.md
rename to new/name.md
index 444..555 100644
--- a/old/name.md
+++ b/new/name.md
@@ -1 +1 @@
-hello
+hello world
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
`;

test("parses modified, added, renamed and binary files", () => {
  const files = parseUnifiedDiff(DIFF);
  assert.deepEqual(
    files.map((f) => [f.path, f.status, f.additions, f.deletions, f.binary]),
    [
      ["src/a.rs", "modified", 2, 1, false],
      ["web/new.ts", "added", 2, 0, false],
      ["new/name.md", "renamed", 1, 1, false],
      ["img.png", "modified", 0, 0, true]
    ]
  );
  assert.equal(files[2]?.oldPath, "old/name.md");
});

test("an empty context line inside a hunk keeps the numbering right", () => {
  const a = must(parseUnifiedDiff(DIFF)[0]);
  assert.deepEqual(
    [...commentableLines(a)].sort((x, y) => x - y),
    [10, 11, 12, 13, 14]
  );
  assert.equal(addedText(a), "let b = 3;\nlet c = 4;");
});

test("touched old lines: replacements are removed lines, insertions anchor to the line before", () => {
  const a = must(parseUnifiedDiff(DIFF)[0]);
  assert.deepEqual(touchedOldLines(a), { removed: [11], anchors: [] });
  const insert = parseUnifiedDiff(`diff --git a/x b/x
--- a/x
+++ b/x
@@ -3,0 +4,2 @@
+one
+two
`);
  assert.deepEqual(touchedOldLines(must(insert[0])), { removed: [], anchors: [3] });
});

test("toRanges groups consecutive lines", () => {
  assert.deepEqual(toRanges([5, 1, 2, 3, 9, 10, 3]), [
    [1, 3],
    [5, 5],
    [9, 10]
  ]);
});

test("config globs", () => {
  assert.ok(globToRegExp("**/Cargo.lock").test("Cargo.lock"));
  assert.ok(globToRegExp("**/Cargo.lock").test("crates/x/Cargo.lock"));
  assert.ok(globToRegExp("crates/workspace-hack/**").test("crates/workspace-hack/src/lib.rs"));
  assert.ok(!globToRegExp("crates/*.rs").test("crates/a/b.rs"));
  assert.ok(globToRegExp("**/*.{png,svg}").test("web-app/public/logo.svg"));
  assert.ok(globToRegExp("crates/api-*/**").test("crates/api-tenancy/src/lib.rs"));
  assert.ok(matchesAny(".github/workflows/ci.yaml", [".github/**"]));
  assert.ok(!matchesAny("src/github.rs", [".github/**"]));
});

test("CODEOWNERS patterns follow gitignore anchoring", () => {
  assert.ok(ownerPatternToRegExp("/crates/authz/").test("crates/authz/src/lib.rs"));
  assert.ok(!ownerPatternToRegExp("/crates/authz/").test("x/crates/authz/src/lib.rs"));
  assert.ok(ownerPatternToRegExp("docs/").test("a/b/docs/x.md"), "trailing slash only: any depth");
  assert.ok(ownerPatternToRegExp("*.md").test("a/b/c.md"));
  assert.ok(
    ownerPatternToRegExp("crates/billing").test("crates/billing/src/x.rs"),
    "a directory match covers its contents"
  );
  assert.ok(
    !ownerPatternToRegExp("crates/billing").test("other/crates/billing/x"),
    "an inner slash anchors"
  );
});
