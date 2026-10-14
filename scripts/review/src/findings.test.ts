import assert from "node:assert/strict";
import { test } from "node:test";
import * as config from "../config.ts";
import {
  applyFilters,
  dedupFindings,
  dropKnown,
  normalizeFinding,
  similarity,
  verdictFor
} from "./findings.ts";
import { lensPick, must, passResult } from "./fixtures.ts";
import { applyVerification, Budget, collectCandidates, schedulePasses } from "./passes.ts";
import type { RawFinding } from "./types.ts";

const raw = (o: RawFinding = {}): RawFinding => ({
  reasoning: "r",
  title: "Full table scan on every request",
  file: "a.rs",
  line: 10,
  category: "performance",
  severity: "should_fix",
  confidence: 0.8,
  fix: "index it",
  rule: "",
  ...o
});
const norm = (o: RawFinding = {}, src = "general") => normalizeFinding(raw(o), src);

test("normalise clamps unknown enums and cleans paths", () => {
  const f = normalizeFinding(
    raw({
      severity: "critical",
      category: "vibes",
      file: "b/src/x.rs:12",
      line: "0",
      confidence: 7
    }),
    "general"
  );
  assert.equal(f.severity, "nit");
  assert.equal(f.category, "correctness");
  assert.equal(f.file, "src/x.rs");
  assert.equal(f.line, 1);
  assert.equal(f.confidence, 1);
  assert.equal(f.rule, null);
});

test("dedup merges the same issue from two passes, keeping the worse severity and both sources", () => {
  const merged = dedupFindings([
    norm({ severity: "nit", confidence: 0.9 }),
    norm({ line: 12, confidence: 0.7 }, "authz")
  ]);
  assert.equal(merged.length, 1);
  const m = must(merged[0]);
  assert.equal(m.severity, "should_fix");
  assert.equal(m.confidence, 0.9);
  assert.deepEqual(m.sources, ["general", "authz"]);
  assert.equal(dedupFindings([norm(), norm({ file: "other.rs" })]).length, 2);
});

test("similarity ignores stop words and punctuation", () => {
  assert.ok(similarity("The `users` table is scanned", "users table scanned on each call") > 0.4);
  assert.equal(similarity("", "x"), 0);
});

test("a candidate restating an open finding is dropped", () => {
  const { kept, known } = dropKnown(
    [norm({ line: 14 })],
    [{ id: "F2", file: "a.rs", line: 10, title: "Full table scan for every request" }]
  );
  assert.equal(kept.length, 0);
  assert.equal(known[0]?.reason, "restates F2");
});

test("filters drop suppressed categories, low confidence, and nits past the cap", () => {
  const nits = [0.95, 0.9, 0.85, 0.8].map((c, i) =>
    norm({ severity: "nit", confidence: c, line: i * 100 })
  );
  const { kept, dropped } = applyFilters(
    [norm({ category: "style" }), norm({ confidence: 0.3 }), norm(), ...nits],
    config
  );
  assert.equal(kept.filter((f) => f.severity === "nit").length, config.MAX_NITS);
  assert.equal(kept.length, 1 + config.MAX_NITS);
  assert.deepEqual(
    dropped.map((d) => d.reason.split(" ")[0]),
    ["category", "confidence", "more"]
  );
});

test("the verdict follows the worst open finding", () => {
  assert.equal(verdictFor([]), "LGTM");
  assert.equal(verdictFor([{ severity: "nit" }]), "LGTM with nits");
  assert.equal(verdictFor([{ severity: "nit" }, { severity: "should_fix" }]), "Needs changes");
  assert.equal(verdictFor([{ severity: "blocking" }]), "Blocking");
});

test("the budget reserves cap plus overshoot and skips lenses that do not fit", () => {
  const budget = new Budget(4, 0.25);
  const lenses = ["authz", "security", "ci"].map(lensPick);
  // 2.25 + 0.75 + 1.0 is exactly the cap; the next lens does not fit.
  const { run, skipped } = schedulePasses({
    lenses,
    needRecheck: true,
    budget,
    caps: { general: 2, recheck: 0.5, lens: 0.75 }
  });
  assert.deepEqual(
    run.map((r) => (r.kind === "lens" ? r.lens.id : r.kind)),
    ["general", "recheck", "authz"]
  );
  assert.deepEqual(
    skipped.map((s) => s.id),
    ["security", "ci"]
  );
  budget.settle(2, 1.1);
  assert.ok(budget.tryReserve(0.75), "settled spend frees the unused reservation");
  assert.throws(
    () =>
      schedulePasses({
        lenses: [],
        needRecheck: false,
        budget: new Budget(1, 0.25),
        caps: { general: 2, recheck: 0.5, lens: 0.75 }
      }),
    /cannot cover/
  );
});

test("candidates come only from finder passes that finished", () => {
  const results = [
    passResult({ name: "general", output: { findings: [raw()], not_reviewed: ["web-app"] } }),
    passResult({ name: "authz", ok: false }),
    passResult({ name: "recheck", output: { results: [] } }),
    passResult({
      name: "security",
      output: { findings: [raw({ title: "", file: "" })], not_reviewed: [] }
    })
  ];
  const { candidates, notReviewed } = collectCandidates(results);
  assert.equal(candidates.length, 1);
  assert.equal(candidates[0]?.cid, "C1");
  assert.deepEqual(notReviewed, ["web-app (general)"]);
});

test("verification confirms, rejects, keeps serious uncertain ones as possible, and folds duplicates", () => {
  const cands = [
    { ...norm({ title: "a" }), cid: "C1" },
    { ...norm({ title: "b" }, "authz"), cid: "C2" },
    { ...norm({ title: "c", severity: "nit" }), cid: "C3" },
    { ...norm({ title: "d", severity: "blocking" }), cid: "C4" },
    { ...norm({ title: "e" }), cid: "C5" }
  ];
  const { confirmed, possible, rejected } = applyVerification(cands, [
    { id: "C1", verdict: "confirmed", severity: "blocking", reason: "" },
    { id: "C2", verdict: "confirmed", severity: "should_fix", reason: "", duplicate_of: "C1" },
    { id: "C3", verdict: "uncertain", severity: "nit", reason: "maybe" },
    { id: "C4", verdict: "uncertain", severity: "blocking", reason: "maybe" },
    { id: "C5", verdict: "rejected", severity: "nit", reason: "guarded upstream" }
  ]);
  assert.deepEqual(
    confirmed.map((c) => [c.cid, c.severity, c.sources]),
    [["C1", "blocking", ["general", "authz"]]]
  );
  assert.deepEqual(
    possible.map((c) => c.cid),
    ["C4"]
  );
  assert.deepEqual(
    rejected.map((c) => c.cid),
    ["C3", "C5"]
  );
});

test("without a verifier only confident candidates are posted", () => {
  const { confirmed, possible } = applyVerification(
    [
      { ...norm({ confidence: 0.9 }), cid: "C1" },
      { ...norm({ confidence: 0.6 }), cid: "C2" }
    ],
    null
  );
  assert.deepEqual(
    confirmed.map((c) => c.cid),
    ["C1"]
  );
  assert.deepEqual(
    possible.map((c) => c.cid),
    ["C2"]
  );
});
