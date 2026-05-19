import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { parse as parseYaml } from "yaml";
import { fmtUsd, PRICING_AS_OF } from "./pricing";
import {
  type CaseResult,
  type CaseRunResult,
  emptyTokens,
  type RunResults,
  type StepDebug,
  type TokenUsage
} from "./types";

/**
 * Add per-run cost into the top-level RunResults so the JSON record is a
 * complete cost report. `cost_usd` on each run already includes its judge
 * spend; we just sum them and stamp the pricing date.
 */
export function summarizeResults(results: RunResults): RunResults {
  let total = 0;
  for (const f of results.flows) {
    for (const c of f.cases) {
      for (const r of c.runs) total += r.cost_usd;
    }
  }
  return { ...results, cost_usd: total, pricing_as_of: PRICING_AS_OF };
}

export function writeResults(results: RunResults, outputPath: string): void {
  const abs = resolve(outputPath);
  mkdirSync(dirname(abs), { recursive: true });
  writeFileSync(abs, JSON.stringify(results, null, 2), "utf-8");
}

export function writeMarkdown(markdown: string, outputPath: string): void {
  const abs = resolve(outputPath);
  mkdirSync(dirname(abs), { recursive: true });
  writeFileSync(abs, markdown, "utf-8");
}

/**
 * If the GITHUB_STEP_SUMMARY env var is set (i.e. running in GitHub Actions),
 * append the markdown summary so it shows up in the run's UI.
 */
export function appendToStepSummary(markdown: string): void {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (!path) return;
  try {
    appendFileSync(path, `${markdown}\n`, "utf-8");
  } catch (err) {
    console.warn(`[reporter] failed to write GITHUB_STEP_SUMMARY: ${formatErr(err)}`);
  }
}

/** Top-level console summary. Concise; the JSON + markdown carry full detail. */
export function printSummary(results: RunResults): void {
  console.log("\n=== Agentic Browser Tests ===");
  console.log(`runtime:      ${results.runtime}`);
  console.log(`duration:     ${(results.duration_ms / 1000).toFixed(1)}s`);
  console.log(
    `cost (USD):   ${fmtUsd(results.cost_usd)}  (pricing as of ${results.pricing_as_of})`
  );

  let totalCases = 0;
  let passedCases = 0;
  const totalTokens = emptyTokens();
  let totalJudgeCalls = 0;

  for (const flow of results.flows) {
    console.log(`\n  ${flow.name}  (${flow.file})`);
    for (const c of flow.cases) {
      totalCases++;
      const passed = casePassed(c);
      if (passed) passedCases++;
      const status = passed ? "PASS" : "FAIL";
      const last = c.runs[c.runs.length - 1];
      const dur = ((last?.duration_ms ?? 0) / 1000).toFixed(1);
      const cost = fmtUsd(last?.cost_usd ?? 0);
      const hits = last?.cache_hits ?? [];
      const hitCount = hits.filter(Boolean).length;
      const cacheNote = hits.length > 0 ? `, cache ${hitCount}/${hits.length}` : "";
      console.log(
        `    [${status}] ${c.name}  (${dur}s, ${last?.step_count ?? 0} steps${cacheNote}, ${cost})`
      );
      for (const er of last?.expect_results ?? []) {
        const sym = er.passed ? "  ok" : "FAIL";
        const detail = er.kind === "assert" ? er.evidence : er.rationale;
        console.log(`        ${sym} [${er.kind}] ${er.claim}${detail ? ` — ${detail}` : ""}`);
      }
      if (last?.error) console.log(`        error: ${last.error}`);
      for (const r of c.runs) {
        addUsage(totalTokens, r.tokens);
        addUsage(totalTokens, r.judge_usage.tokens);
        totalJudgeCalls += r.judge_usage.calls;
      }
    }
  }

  console.log(
    `\n${passedCases}/${totalCases} cases passed.  tokens: ${totalTokens.input} in / ${totalTokens.cached_input} cached / ${totalTokens.cache_creation} cache-write / ${totalTokens.output} out  (${totalJudgeCalls} judge calls)\n`
  );
}

/**
 * Markdown summary suitable for $GITHUB_STEP_SUMMARY or a sibling .md file
 * next to the output JSON. Optimized to be readable in a PR check page and
 * grep-friendly enough for an agent triaging a failure.
 */
export function buildMarkdown(results: RunResults): string {
  const lines: string[] = [];
  lines.push(`## Agentic Browser Tests — \`${results.runtime}\``);
  lines.push("");
  const passed = countPassed(results);
  lines.push(
    `**${passed.passed}/${passed.total}** cases passed · **${(results.duration_ms / 1000).toFixed(1)}s** total · **${fmtUsd(results.cost_usd)}** (pricing as of ${results.pricing_as_of})`
  );
  lines.push("");
  lines.push(
    "| flow | case | run | result | duration | input | cached | cache-write | output | $ | cache hits |"
  );
  lines.push("|---|---|---|---|---|---|---|---|---|---|---|");
  for (const flow of results.flows) {
    for (const c of flow.cases) {
      for (let i = 0; i < c.runs.length; i++) {
        const r = c.runs[i];
        const result = r.passed ? "✅" : "❌";
        const dur = `${(r.duration_ms / 1000).toFixed(1)}s`;
        const hits = r.cache_hits.filter(Boolean).length;
        lines.push(
          `| ${flow.name} | ${c.name} | ${i + 1}/${c.runs.length} | ${result} | ${dur} | ${r.tokens.input} | ${r.tokens.cached_input} | ${r.tokens.cache_creation} | ${r.tokens.output} | ${fmtUsd(r.cost_usd)} | ${hits}/${r.cache_hits.length} |`
        );
      }
    }
  }
  lines.push("");

  // Per-step debug for any failing run, plus all step_debug for context.
  for (const flow of results.flows) {
    for (const c of flow.cases) {
      for (let i = 0; i < c.runs.length; i++) {
        const r = c.runs[i];
        lines.push(`### ${flow.name} → ${c.name} (run ${i + 1})`);
        if (r.error) lines.push(`- **step error:** \`${r.error}\``);
        lines.push("");
        lines.push(stepDebugTable(r.step_debug));
        lines.push("");
        if (r.expect_results.length > 0) {
          lines.push("**Expectations**");
          for (const er of r.expect_results) {
            const sym = er.passed ? "✅" : "❌";
            const detail = er.kind === "assert" ? er.evidence : er.rationale;
            lines.push(`- ${sym} [${er.kind}] ${er.claim}${detail ? ` — ${detail}` : ""}`);
          }
          lines.push("");
        }
        if (r.judge_usage.calls > 0) {
          lines.push(
            `**Judge** (${r.judge_usage.model}): ${r.judge_usage.calls} call(s), ${r.judge_usage.tokens.input}/${r.judge_usage.tokens.cached_input} in/cached, ${r.judge_usage.tokens.output} out, ${fmtUsd(r.judge_usage.cost_usd)}`
          );
          lines.push("");
        }
        if (r.trace_path) {
          lines.push(`**Trace:** [\`${r.trace_path}\`](${r.trace_path})`);
          lines.push("");
        }
      }
    }
  }

  const budgetSection = buildBudgetSection(results);
  if (budgetSection.length > 0) {
    lines.push(...budgetSection);
  }

  return lines.join("\n");
}

interface FlowBudgetEntry {
  flow: string;
  warm_usd?: number;
  cold_usd?: number;
}

function loadBudgets(): FlowBudgetEntry[] {
  // Optional file. When absent, the cost-budget section is skipped.
  // Resolved relative to the runner's CWD via the same strategy used by
  // discoverFlows in cli.ts: we walk upward from this module path.
  // The reporter is imported in tests too, so be defensive on file
  // existence.
  const candidates = [
    resolve(process.cwd(), "tests/agentic/flows/_budgets.yml"),
    resolve(process.cwd(), "web-app/tests/agentic/flows/_budgets.yml")
  ];
  for (const path of candidates) {
    if (!existsSync(path)) continue;
    try {
      const parsed = parseYaml(readFileSync(path, "utf-8"));
      if (Array.isArray(parsed)) return parsed as FlowBudgetEntry[];
    } catch (err) {
      console.warn(
        `[reporter] could not load ${path}: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }
  return [];
}

function buildBudgetSection(results: RunResults): string[] {
  const budgets = loadBudgets();
  if (budgets.length === 0) return [];

  const lines: string[] = ["", "## Cost budget"];
  lines.push("");
  lines.push("| flow | budget (USD) | observed (USD) | status |");
  lines.push("|---|---|---|---|");

  for (const flow of results.flows) {
    const totalCost = flow.cases.reduce(
      (s, c) => s + c.runs.reduce((rs, r) => rs + r.cost_usd, 0),
      0
    );
    const allWarm = flow.cases.every((c) =>
      c.runs.every((r) => r.cache_hits.length > 0 && r.cache_hits.every(Boolean))
    );
    const budget = budgets.find((b) => b.flow === flow.name);
    if (!budget) continue;
    const ceiling = allWarm ? budget.warm_usd : budget.cold_usd;
    if (ceiling === undefined) continue;
    const overBudget = totalCost > ceiling;
    const status = overBudget ? "⚠️ over" : "✅ within";
    lines.push(
      `| ${flow.name} | ${fmtUsd(ceiling)} ${allWarm ? "(warm)" : "(cold)"} | ${fmtUsd(totalCost)} | ${status} |`
    );
  }
  return lines;
}

function stepDebugTable(steps: StepDebug[]): string {
  const lines: string[] = [];
  lines.push(
    "| # | kind | step | iter | model | dur | tokens (in/cached/cw/out) | $ | snap (calls/hits/bytes) | tools | cache | err |"
  );
  lines.push("|---|---|---|---|---|---|---|---|---|---|---|---|");
  for (const s of steps) {
    const text = s.text.length > 80 ? `${s.text.slice(0, 80)}…` : s.text;
    const dur = `${(s.duration_ms / 1000).toFixed(1)}s`;
    const tok = `${s.tokens.input}/${s.tokens.cached_input}/${s.tokens.cache_creation}/${s.tokens.output}`;
    const snap = `${s.snapshot_calls}/${s.snapshot_cache_hits}/${s.snapshot_bytes}`;
    const tools = s.tool_calls.length === 0 ? "—" : s.tool_calls.map((t) => t.name).join(",");
    const cache = s.from_cache ? "hit" : "miss";
    const err = s.error ? `\`${s.error.slice(0, 60)}\`` : "";
    const escalated = s.escalated ? " ⚠️esc" : "";
    lines.push(
      `| ${s.step_index} | ${s.kind} | ${text.replace(/\|/g, "\\|")} | ${s.iterations}${escalated} | ${s.model ?? "—"} | ${dur} | ${tok} | ${fmtUsd(s.cost_usd)} | ${snap} | ${tools} | ${cache} | ${err} |`
    );
  }
  return lines.join("\n");
}

function countPassed(results: RunResults): { passed: number; total: number } {
  let total = 0;
  let passed = 0;
  for (const f of results.flows) {
    for (const c of f.cases) {
      total++;
      if (casePassed(c)) passed++;
    }
  }
  return { passed, total };
}

function casePassed(c: CaseResult): boolean {
  return c.runs.every((r: CaseRunResult) => r.passed && r.expect_results.every((e) => e.passed));
}

function addUsage(into: TokenUsage, from: TokenUsage): void {
  into.input += from.input;
  into.cached_input += from.cached_input;
  into.cache_creation += from.cache_creation;
  into.output += from.output;
}

function formatErr(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
