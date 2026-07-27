#!/usr/bin/env node
// Format agentic run artefacts into a markdown body suitable for a PR
// comment. Two independent sections, either of which can be empty:
//
//   1. Healing events (written by the bespoke runtime when every recorded
//      selector strategy for a cached action fails on replay).
//   2. Failure detail — the erroring steps from the results JSON. CI job
//      logs and the results artifact are served from Azure blob storage,
//      which a default-deny network policy can block, so mirroring the
//      failing `step_debug` entries into the PR keeps a failure
//      diagnosable from the GitHub API alone.
//
// Usage:
//   node agentic-healing-comment.mjs <bucket> <healing-json-path> [results-json-path]
// Writes the markdown body to stdout. Exits 0 with no output if there is
// nothing to report — the caller can `[ -z "$BODY" ]` to skip posting.

import { existsSync, readFileSync } from "node:fs";

const [, , bucket, healingPath, resultsPathArg] = process.argv;
if (!bucket || !healingPath) {
  console.error(
    "usage: agentic-healing-comment.mjs <bucket> <healing-json-path> [results-json-path]"
  );
  process.exit(2);
}

const readJson = (p) => {
  if (!p || !existsSync(p)) return undefined;
  const raw = readFileSync(p, "utf8");
  if (!raw.trim()) return undefined;
  try {
    return JSON.parse(raw);
  } catch {
    return undefined;
  }
};

const escapePipes = (s) => String(s ?? "—").replace(/\|/g, "\\|");
const oneLine = (s, n) =>
  String(s ?? "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, n);

// --- Section 2: failure detail -------------------------------------------
// Default to the path ci writes (`--output ../agentic-results-<bucket>.json`
// from web-app/, i.e. repo root) so the caller can omit the argument.
const results = readJson(resultsPathArg ?? `agentic-results-${bucket}.json`);
const failureLines = [];
for (const flow of results?.flows ?? []) {
  for (const kase of flow.cases ?? []) {
    for (const [i, run] of (kase.runs ?? []).entries()) {
      const badSteps = (run.step_debug ?? []).filter((s) => s.error);
      if (!run.error && badSteps.length === 0) continue;
      failureLines.push(`**\`${flow.name}\` → ${kase.name}** (run ${i + 1})`);
      failureLines.push("");
      if (run.error) {
        failureLines.push("```");
        failureLines.push(oneLine(run.error, 600));
        failureLines.push("```");
        failureLines.push("");
      }
      if (badSteps.length) {
        failureLines.push("| step | kind | iters | error | step text |");
        failureLines.push("|---|---|---|---|---|");
        for (const s of badSteps) {
          failureLines.push(
            `| ${s.step_index ?? "—"} | ${s.kind ?? "—"} | ${s.iterations ?? "—"} | ${escapePipes(oneLine(s.error, 200))} | ${escapePipes(oneLine(s.text, 90))} |`
          );
        }
        failureLines.push("");
      }
    }
  }
}

// --- Section 1: healing events -------------------------------------------
const events = readJson(healingPath);
const healingLines = [];
if (Array.isArray(events) && events.length > 0) {
  healingLines.push(
    `### Agentic healing — \`${bucket}\``,
    "",
    "A previously cached selector strategy failed all fallbacks on replay.",
    "The runtime did an intent-aware redrive and staged the new recording.",
    "",
    "| case | step | action | old → new (kind) | intent |",
    "|---|---|---|---|---|"
  );
}

const lines = [];
if (failureLines.length) {
  lines.push(`### Agentic failure detail — \`${bucket}\``, "", ...failureLines);
}
lines.push(...healingLines);
if (lines.length === 0) process.exit(0);

if (healingLines.length) {
  for (const e of events) {
    const oldS = escapePipes(e.drift?.old_primary);
    const newS = escapePipes(e.drift?.new_primary);
    const oldK = e.drift?.old_kind ?? "—";
    const newK = e.drift?.new_kind ?? "—";
    const intent = escapePipes((e.drift?.intent ?? "").slice(0, 80));
    lines.push(
      `| ${e.case} | ${e.step_index} | ${e.action_index} | \`${oldS}\` → \`${newS}\` (${oldK} → ${newK}) | ${intent} |`
    );
  }

  lines.push("");
  lines.push(
    `**Promote to ground truth:** \`pnpm test:agentic --accept-healing ${bucket}\` then commit the cache diff.`
  );
}

process.stdout.write(lines.join("\n") + "\n");
