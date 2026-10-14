// Scoring a review against a golden case: did it find the known bug, at the right
// severity, and how much else did it say?

import { severityRank } from "./findings.ts";
import type { Finding, Possible, Severity } from "./types.ts";

export interface Expected {
  file: string;
  line: number;
  tolerance?: number;
  severity?: Severity;
  what: string;
}

export interface GoldenCase {
  id: string;
  pr: number;
  base: string;
  head: string;
  source?: string;
  expect?: Expected[];
}

export type HitResult = "found" | "found-low-severity" | "possible-only" | "missed" | "error";

export interface Hit {
  what: string;
  result: HitResult;
  as?: string;
}

export interface Score {
  id: string;
  pr: number;
  hits: Hit[];
  extra: string[];
  cleanFalseAlarms: number;
  cost: number;
  error?: string;
}

type Located = Pick<Finding, "file" | "line">;

const near = (f: Located, e: Expected): boolean =>
  f.file === e.file && Math.abs(f.line - e.line) <= (e.tolerance ?? 10);

export function scoreCase(
  kase: GoldenCase,
  {
    findings,
    possible,
    cost
  }: {
    findings: readonly Pick<Finding, "file" | "line" | "severity" | "title">[];
    possible: readonly Pick<Possible, "file" | "line" | "title">[];
    cost: number;
  }
): Score {
  const expected = kase.expect ?? [];
  const matched = new Set<(typeof findings)[number]>();
  const hits = expected.map((e): Hit => {
    const f = findings.find((x) => near(x, e));
    if (f) {
      matched.add(f);
      return {
        what: e.what,
        result:
          severityRank(f.severity) >= severityRank(e.severity ?? "nit")
            ? "found"
            : "found-low-severity",
        as: `${f.severity}: ${f.title}`
      };
    }
    const p = possible.find((x) => near(x, e));
    return p
      ? { what: e.what, result: "possible-only", as: p.title }
      : { what: e.what, result: "missed" };
  });
  const serious = findings.filter((f) => severityRank(f.severity) >= severityRank("should_fix"));
  return {
    id: kase.id,
    pr: kase.pr,
    hits,
    extra: findings
      .filter((f) => !matched.has(f))
      .map((f) => `${f.severity}: ${f.file}:${f.line} ${f.title}`),
    cleanFalseAlarms: expected.length === 0 ? serious.length : 0,
    cost
  };
}

export interface Summary {
  cases: number;
  expected: number;
  found: number;
  foundLow: number;
  possibleOnly: number;
  missed: number;
  recall: number | null;
  cleanFalseAlarms: number;
  extraFindings: number;
  cost: number;
}

export function summarize(scores: readonly Score[]): Summary {
  const hits = scores.flatMap((s) => s.hits);
  const count = (r: HitResult) => hits.filter((h) => h.result === r).length;
  return {
    cases: scores.length,
    expected: hits.length,
    found: count("found"),
    foundLow: count("found-low-severity"),
    possibleOnly: count("possible-only"),
    missed: count("missed"),
    recall: hits.length
      ? Math.round((100 * (count("found") + count("found-low-severity"))) / hits.length)
      : null,
    cleanFalseAlarms: scores.reduce((n, s) => n + s.cleanFalseAlarms, 0),
    extraFindings: scores.reduce((n, s) => n + s.extra.length, 0),
    cost: Math.round(scores.reduce((n, s) => n + s.cost, 0) * 100) / 100
  };
}
