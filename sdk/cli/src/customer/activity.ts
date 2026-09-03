/**
 * Delivery activity — merged pull requests per customer, read from GitHub at
 * query time.
 *
 * TWO QUERIES, AND THEY DIFFER IN KIND:
 *
 *   the customer's OWN repo   the repo IS the customer, so every merged PR
 *                             counts and nothing has to be marked. Exact,
 *                             retroactive, forgery-proof.
 *   our SHARED repos          returns their whole PR stream, so repo scope
 *                             means nothing here. Attribution is per-PR: a
 *                             `Customer: <name>` line the session writes into
 *                             the body. GitHub search TOKENIZES, so a hit is a
 *                             CANDIDATE; the local re-filter below decides.
 *
 * THE SAFETY RULE is `gh.ts`'s, for the same reason: an empty answer here is
 * indistinguishable from a broken call, a merged PR that never appears looks
 * like work that never happened, and nobody audits a record for absence.
 */

import { ghJson, refuseIfAtLimit } from "../github/gh.js";
import { CliError, ExitCode } from "../util/errors.js";
import { attributionLine, SHARED_REPOS } from "./repos.js";

export interface ActivityRecord {
  repo: string;
  number: number;
  title: string;
  url: string;
  author: string;
  mergedAt: string;
  /** `own` for the customer's repo, `shared` for one of ours. */
  via: "own" | "shared";
  /** `YYYY-MM`, derived here so a reader of the file can group without this code. */
  month: string;
}

/**
 * The ceiling `gh search prs` is asked for, and THE DEFAULT IS THE CEILING.
 *
 * The refusal below fires whenever the count EQUALS the limit, and it cannot
 * tell "exactly N, complete" from "truncated at N". Any default below gh's own
 * maximum therefore turns some perfectly ordinary repo into a permanent
 * refusal — which is not hypothetical: the first real run of this command
 * refused because that customer's repo had exactly 200 merged pull requests
 * and the default was 200. At the ceiling, a refusal means a result set
 * genuinely larger than anything gh will return, where `--since` really is the
 * answer.
 */
function searchLimit(): number {
  const raw = Number(process.env.OXYC_SEARCH_LIMIT ?? "1000");
  return Number.isFinite(raw) && raw > 0 ? Math.min(raw, 1000) : 1000;
}

/**
 * The attribution matcher — and THE ONE THAT LOOKS RIGHT AND MATCHES NOTHING.
 *
 * The obvious spelling is an exact `^Customer: <name>$` body line. Transcribed
 * into jq that filter can NEVER fire, because jq's `^` and `$` are STRING
 * anchors, not line anchors, and its "m" flag only makes `.` match a newline.
 * Every cross-repo PR would have been dropped and the command would have
 * reported an honest-looking zero forever.
 *
 * JavaScript has a real multiline flag, so the trap does not exist here — but
 * the rest of the pattern is each its own small refusal and all of them are
 * kept:
 *
 *   (^|\r?\n)      a real line start. GitHub bodies are \n today, but the API
 *                  has always been free to hand back \r\n, and a stray CR
 *                  would sit between the name and the anchor and silently miss.
 *   [Cc]ustomer:   the label a human repairing a body by hand might lowercase.
 *   [ \t]*         spacing nobody should have to get exactly right.
 *   <name>         REGEX-ESCAPED: a repo name may contain `.`, which is a
 *                  metacharacter, and an unescaped one matches any character.
 *   [ \t\r]*(\n|$) trailing whitespace, and the CR again.
 *
 * What it deliberately does NOT match: the name inside a prose sentence, an
 * indented `- Customer: x` list item, or `x-staging` when the customer is `x`.
 */
export function attributionPattern(customerName: string): RegExp {
  const escaped = customerName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|\\r?\\n)[Cc]ustomer:[ \\t]*${escaped}[ \\t\\r]*(\\n|$)`);
}

interface SearchHit {
  repository?: { nameWithOwner?: string };
  number?: number;
  title?: string;
  url?: string;
  author?: { login?: string };
  body?: string;
  closedAt?: string;
}

/** One guarded `gh search prs`. Every failure mode is loud; none returns empty. */
function search(term: string | undefined, since: string | undefined, repos: string[]): SearchHit[] {
  const limit = searchLimit();
  const args = [
    "search",
    "prs",
    "--merged",
    "--limit",
    String(limit),
    "--json",
    "repository,number,title,url,author,body,closedAt"
  ];
  for (const repo of repos) args.push("--repo", repo);
  if (since) args.push("--merged-at", `>=${since}`);
  // The term may be empty — that is the customer's own repo, where the repo
  // filter IS the query and there is nothing to search for.
  if (term) args.push(term);

  const hits = ghJson<SearchHit[]>(args, `merged pull requests in ${repos.join(", ")}`);
  refuseIfAtLimit(hits.length, limit, `the pull-request search for ${repos.join(", ")}`);
  return hits;
}

function normalise(hit: SearchHit, via: "own" | "shared"): ActivityRecord | undefined {
  const mergedAt = hit.closedAt;
  if (!mergedAt) return undefined;
  return {
    repo: hit.repository?.nameWithOwner ?? "",
    number: hit.number ?? 0,
    title: hit.title ?? "",
    url: hit.url ?? "",
    author: hit.author?.login ?? "",
    mergedAt,
    via,
    // Derived HERE rather than at render time: it is the bucketing key, and a
    // reader who never runs this code can still group the file.
    month: mergedAt.slice(0, 7)
  };
}

export interface ActivityOptions {
  /** The customer's repo name, which is also the attribution token. */
  name: string;
  /** `<org>/<name>` for the customer's own repo. */
  ownRepo: string;
  /** Only pull requests merged on or after this date (YYYY-MM-DD). */
  since?: string;
  /** Extra repos to search, beyond the shared four. */
  extraRepos?: string[];
}

/**
 * Both queries, merged.
 *
 * The customer's own repo needs no attribution; the shared repos are
 * RE-FILTERED LOCALLY because a GitHub search hit is only a candidate — the
 * index tokenises, so searching for `Customer: acme` also returns PRs that
 * merely mention either word.
 */
export function collectActivity(opts: ActivityOptions): ActivityRecord[] {
  const own = search(undefined, opts.since, [opts.ownRepo])
    .map((hit) => normalise(hit, "own"))
    .filter((r): r is ActivityRecord => r !== undefined);

  const sharedRepos = [...SHARED_REPOS, ...(opts.extraRepos ?? [])];
  const pattern = attributionPattern(opts.name);
  const shared = search(attributionLine(opts.name), opts.since, sharedRepos)
    .filter((hit) => pattern.test(hit.body ?? ""))
    .map((hit) => normalise(hit, "shared"))
    .filter((r): r is ActivityRecord => r !== undefined);

  return [...own, ...shared].sort((a, b) => a.mergedAt.localeCompare(b.mergedAt));
}

/**
 * Attributions naming somebody who is NOT this customer, among the candidates
 * already fetched.
 *
 * OPPORTUNISTIC, and said out loud rather than sold as a sweep: these are
 * bodies the search returned while looking for THIS customer, so the only ones
 * visible are those that happened to share tokens. A typo nobody's search
 * collides with stays invisible. Finding a real one usually means a typo, or a
 * repo that lost its topic — both worth a human's eye, and neither worth a
 * second firehose query against the bare word "Customer", which comes back at
 * the cap.
 */
export function foreignAttributions(records: SearchHit[], name: string): string[] {
  const found = new Set<string>();
  for (const hit of records) {
    for (const match of (hit.body ?? "").matchAll(/(?:^|\r?\n)[Cc]ustomer:[ \t]*([^\s\r\n]+)/g)) {
      const named = match[1];
      if (named && named !== name) found.add(named);
    }
  }
  return [...found].sort();
}

/** The distinct months a result set touches, oldest first. */
export function monthsIn(records: ActivityRecord[]): string[] {
  return [...new Set(records.map((r) => r.month))].sort();
}

/**
 * Markdown for one month.
 *
 * NO GENERATED-ON TIMESTAMP, deliberately. The record is keyed by (repo,
 * number) so a re-run is idempotent; a date stamped into the markdown would
 * make every re-run a diff, and a file that churns without changing meaning is
 * one people stop reading. Git already records when it was written.
 */
export function renderMonth(month: string, display: string, records: ActivityRecord[]): string {
  const forMonth = records.filter((r) => r.month === month);
  const lines = [`# ${display} — ${month}`, ""];
  for (const via of ["own", "shared"] as const) {
    const group = forMonth.filter((r) => r.via === via);
    if (group.length === 0) continue;
    lines.push(via === "own" ? "## In their repo" : "## In our repos", "");
    for (const record of group) {
      lines.push(
        `- [${record.repo}#${record.number}](${record.url}) — ${record.title} (@${record.author})`
      );
    }
    lines.push("");
  }
  return lines.join("\n");
}

/** One JSON object per line — the authoritative half, byte-stable across re-runs. */
export function renderJsonl(month: string, records: ActivityRecord[]): string {
  return records
    .filter((r) => r.month === month)
    .map((r) =>
      JSON.stringify({
        repo: r.repo,
        number: r.number,
        title: r.title,
        url: r.url,
        author: r.author,
        merged_at: r.mergedAt,
        via: r.via,
        month: r.month
      })
    )
    .join("\n");
}

/** A month with no merged PRs produces NO FILE rather than an empty one. */
export function requireNonEmpty(records: ActivityRecord[], name: string): void {
  if (records.length > 0) return;
  throw new CliError(`no merged pull requests found for ${name}`, {
    code: ExitCode.NOT_FOUND,
    hint: 'an empty file is a claim, and "we did nothing" is not one this record is entitled to make'
  });
}
