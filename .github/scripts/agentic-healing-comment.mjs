#!/usr/bin/env node
// Format a healing-events JSON file (written by the bespoke runtime when
// every recorded selector strategy for a cached action fails on replay)
// into a markdown table suitable for a PR comment.
//
// Usage:
//   node agentic-healing-comment.mjs <bucket> <healing-json-path>
// Writes the markdown body to stdout. Exits 0 with no output if there
// are no events to report — the caller can `[ -z "$BODY" ]` to skip
// posting.

import { existsSync, readFileSync } from "node:fs";

const [, , bucket, healingPath] = process.argv;
if (!bucket || !healingPath) {
  console.error("usage: agentic-healing-comment.mjs <bucket> <healing-json-path>");
  process.exit(2);
}

if (!existsSync(healingPath)) process.exit(0);
const raw = readFileSync(healingPath, "utf8");
if (!raw.trim()) process.exit(0);

let events;
try {
  events = JSON.parse(raw);
} catch {
  process.exit(0);
}
if (!Array.isArray(events) || events.length === 0) process.exit(0);

const escapePipes = (s) => String(s ?? "—").replace(/\|/g, "\\|");

const lines = [
  `### Agentic healing — \`${bucket}\``,
  "",
  "A previously cached selector strategy failed all fallbacks on replay.",
  "The runtime did an intent-aware redrive and staged the new recording.",
  "",
  "| case | step | action | old → new (kind) | intent |",
  "|---|---|---|---|---|"
];

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

process.stdout.write(lines.join("\n") + "\n");
