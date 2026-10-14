#!/usr/bin/env node
// The only writer of the two merge statuses, run by .github/workflows/review-gate.yaml
// from the default branch (the review job itself holds no status permission):
//   --gate  `review-gate` from the review state, after a push, a label change, or the
//           review run finishing
//   --risk  `high-risk-approval` from the PR's files and reviews

import { parseArgs } from "node:util";
import * as config from "./config.ts";
import { parseCodeowners } from "./src/codeowners.ts";
import { evaluateGate, evaluateRisk } from "./src/gate.ts";
import * as gh from "./src/github.ts";
import { decodeState, findSummaryComment } from "./src/state.ts";
import type { CommitStatus } from "./src/types.ts";

const { values: args } = parseArgs({
  options: {
    repo: { type: "string", default: process.env.GITHUB_REPOSITORY },
    pr: { type: "string" },
    gate: { type: "boolean", default: false },
    risk: { type: "boolean", default: false },
    "run-url": { type: "string", default: process.env.RUN_URL },
    "dry-run": { type: "boolean", default: false },
    // The conclusion of the review workflow run that triggered this, if one did.
    "review-conclusion": { type: "string", default: process.env.REVIEW_CONCLUSION || "" },
    "review-head": { type: "string", default: process.env.REVIEW_HEAD_SHA || "" }
  }
});

const repo = args.repo ?? "";
const pr = Number(args.pr);
if (!repo || !Number.isInteger(pr) || pr <= 0)
  throw new Error("--repo and --pr <number> are required");
const meta = await gh.getPr(repo, pr);
const target = args["run-url"] || undefined;

async function report(context: string, result: CommitStatus): Promise<void> {
  console.log(`${context}: ${result.state} · ${result.description}`);
  if (!args["dry-run"])
    await gh.setStatus(repo, meta.head.sha, {
      state: result.state,
      context,
      description: result.description,
      target_url: target
    });
}

if (args.gate) {
  const comments = await gh.listIssueComments(repo, pr);
  const state = decodeState(
    findSummaryComment(comments, config.SUMMARY_MARKER, config.REVIEWER_LOGINS)?.body,
    config.STATE_VERSION
  );
  const labels = meta.labels.map((l) => l.name);
  await report(
    config.GATE_CONTEXT,
    evaluateGate({
      state,
      headSha: meta.head.sha,
      labels,
      draft: meta.draft,
      overrideLabel: config.OVERRIDE_LABEL,
      // Only a failed run for this very head: an older head's failure says nothing here.
      reviewJobFailed:
        args["review-conclusion"] === "failure" && args["review-head"] === meta.head.sha
    })
  );
}

if (args.risk) {
  const files = (await gh.listPrFiles(repo, pr)).flatMap((f) =>
    f.previous_filename ? [f.filename, f.previous_filename] : [f.filename]
  );
  const reviews = await gh.listReviews(repo, pr);
  // The base branch's CODEOWNERS: a PR cannot make its own author an owner.
  const owners = await gh
    .getFile(repo, ".github/CODEOWNERS", meta.base.sha ?? meta.base.ref)
    .catch(() => "");
  const rules = parseCodeowners(owners);
  await report(
    config.RISK_CONTEXT,
    evaluateRisk({ files, reviews, author: meta.user.login, rules, highRisk: config.HIGH_RISK })
  );
}
