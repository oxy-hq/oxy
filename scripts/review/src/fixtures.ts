// Typed builders for the tests: each fills the fields a test does not care about.

import type {
  Candidate,
  Finding,
  LensPick,
  PassResult,
  PrMeta,
  ReviewState,
  Round
} from "./types.ts";

/** The value, or a thrown error naming what was missing: narrows `T | undefined` in a test. */
export function must<T>(value: T | null | undefined, what = "value"): T {
  if (value === null || value === undefined) throw new Error(`expected a ${what}`);
  return value;
}

export const finding = (o: Partial<Finding> = {}): Finding => ({
  id: "F1",
  title: "A finding",
  file: "a.rs",
  line: 1,
  end_line: null,
  severity: "should_fix",
  category: "correctness",
  confidence: 0.8,
  lens: "general",
  sources: ["general"],
  rule: null,
  status: "open",
  first: "aaaaaaa",
  ...o
});

export const candidate = (o: Partial<Candidate> = {}): Candidate => ({
  title: "A candidate",
  file: "a.rs",
  line: 1,
  end_line: null,
  category: "correctness",
  severity: "should_fix",
  confidence: 0.8,
  reasoning: "",
  fix: "",
  rule: null,
  sources: ["general"],
  ...o
});

export const prMeta = (o: Partial<PrMeta> = {}): PrMeta => ({
  number: 1,
  title: "feat: a change",
  body: null,
  draft: false,
  user: { login: "dev", type: "User" },
  labels: [],
  head: { sha: "head" },
  base: { ref: "main" },
  ...o
});

export const reviewState = (o: Partial<ReviewState> = {}): ReviewState => ({
  v: 2,
  pr: 1,
  head: null,
  base: null,
  status: "complete",
  verdict: null,
  findings: [],
  possible: [],
  rounds: [],
  next: 1,
  ...o
});

export const round = (o: Partial<Round> = {}): Round => ({
  sha: "aaaaaaa",
  at: "2026-01-01T00:00:00.000Z",
  mode: "full",
  status: "complete",
  verdict: null,
  cost: 0,
  ms: 0,
  lines: 0,
  lenses: [],
  added: [],
  closed: [],
  ...o
});

export const passResult = <T = unknown>(
  o: Partial<PassResult<T>> & { name: string }
): PassResult<T> => ({
  ok: true,
  subtype: "success",
  output: null,
  cost: 0,
  ms: 0,
  turns: 0,
  tools: {},
  error: "",
  ...o
});

export const lensPick = (id: string): LensPick => ({ id, prompt: `${id}.md`, reason: "test" });
