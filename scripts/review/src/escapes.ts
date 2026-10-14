// Bugs the review let through: for each merged `fix:` PR, blame the lines it changed back
// to the PR that wrote them, then ask whether that PR's review flagged the spot.
// A heuristic, so the report calls these candidates.

import { parseUnifiedDiff, toRanges, touchedOldLines } from "./diff.ts";
import type { Git } from "./exec.ts";
import type { PrRecord } from "./metrics.ts";
import type { FindingStatus } from "./types.ts";

const PR_REF = /\(#(\d+)\)\s*$/;
const MAX_RANGES_PER_FILE = 20;
const LINE_SLACK = 15;

export interface Blamed {
  sha: string;
  orig: number;
  summary: string;
}

export interface Intro {
  pr: number;
  file: string;
  lines: number[];
  kind: "changed" | "inserted-near";
}

export interface Culprit {
  pr: number;
  lines: number;
  files: { file: string; lines: number[] }[];
}

export interface Flag {
  flagged: boolean | null;
  finding?: string;
  status?: FindingStatus;
  legacy?: boolean;
  unreviewed?: boolean;
}

/** One line of the report's escaped-bug section: a fix, and the PR it most likely fixes. */
export type Escape =
  | { broad: true; fix: number; fixTitle: string }
  | ({
      broad?: false;
      fix: number;
      fixTitle: string;
      intro: number;
      files: string[];
      file?: string;
    } & Flag);

export const isFixTitle = (title: string | null | undefined): boolean =>
  /^fix(\([^)]*\))?!?:/i.test((title ?? "").trim());

/** `git blame --porcelain` → [{ sha, orig, summary }], one per blamed line. */
export function parseBlame(text: string): Blamed[] {
  const out: { sha: string; orig: number }[] = [];
  const summaries = new Map<string, string>();
  let current: { sha: string; orig: number } | null = null;
  for (const line of text.split("\n")) {
    const head = line.match(/^([0-9a-f]{40}) (\d+) (\d+)/);
    if (head?.[1]) {
      current = { sha: head[1], orig: Number(head[2]) };
      out.push(current);
    } else if (line.startsWith("summary ") && current) {
      summaries.set(current.sha, line.slice(8));
    }
  }
  return out.map((b) => ({ ...b, summary: summaries.get(b.sha) ?? "" }));
}

async function blameRange(
  git: Git,
  rev: string,
  path: string,
  [a, b]: [number, number]
): Promise<Blamed[]> {
  const text = await git(["blame", "--porcelain", "-w", "-L", `${a},${b}`, rev, "--", path]).catch(
    () => ""
  );
  return parseBlame(text);
}

/** Which earlier PRs wrote the lines one fix commit changed. */
export async function introducedBy({
  git,
  fix
}: {
  git: Git;
  fix: { sha: string; number: number };
}): Promise<Intro[]> {
  const diff = parseUnifiedDiff(
    await git(["diff", "-U0", "--no-color", "-M", `${fix.sha}^`, fix.sha])
  );
  const found = new Map<string, Intro>();
  for (const file of diff) {
    if (file.status === "added" || file.binary) continue;
    const { removed, anchors } = touchedOldLines(file);
    const ranges = toRanges([...removed, ...anchors]).slice(0, MAX_RANGES_PER_FILE);
    for (const range of ranges) {
      for (const b of await blameRange(git, `${fix.sha}^`, file.oldPath, range)) {
        const pr = Number(b.summary.match(PR_REF)?.[1]);
        if (!pr || pr === fix.number) continue;
        const key = `${pr}:${file.oldPath}`;
        const intro = found.get(key) ?? {
          pr,
          file: file.oldPath,
          lines: [],
          kind: removed.length ? "changed" : "inserted-near"
        };
        intro.lines.push(b.orig);
        found.set(key, intro);
      }
    }
  }
  return [...found.values()];
}

/**
 * Reduce a fix's blame to the PRs it most plausibly fixes: the one or two that wrote
 * the largest share of the lines it changed. A fix touching more than `maxLines` old
 * lines is a refactor wearing a `fix:` title and is left out.
 */
export function attribute(
  intros: readonly Intro[],
  {
    maxLines = 300,
    minShare = 0.3,
    maxIntros = 2
  }: { maxLines?: number; minShare?: number; maxIntros?: number } = {}
): { broad: boolean; culprits: Culprit[] } {
  const byPr = new Map<number, Culprit>();
  for (const i of intros) {
    const e = byPr.get(i.pr) ?? { pr: i.pr, lines: 0, files: [] };
    e.lines += i.lines.length;
    e.files.push({ file: i.file, lines: i.lines });
    byPr.set(i.pr, e);
  }
  const total = [...byPr.values()].reduce((n, e) => n + e.lines, 0);
  if (!total || total > maxLines) return { broad: total > maxLines, culprits: [] };
  const ranked = [...byPr.values()].sort((a, b) => b.lines - a.lines);
  const culprits = ranked
    .filter((e, i) => i === 0 || e.lines >= minShare * total)
    .slice(0, maxIntros);
  return { broad: false, culprits };
}

/** Did the introducing PR's review point at the place the fix later changed? */
export function wasFlagged(
  record: Pick<PrRecord, "review" | "findings">,
  file: string,
  lines: readonly number[]
): Flag {
  if (record.review === "v2") {
    const hit = record.findings.find(
      (f) => f.file === file && lines.some((l) => Math.abs(l - f.line) <= LINE_SLACK)
    );
    return hit ? { flagged: true, finding: hit.id, status: hit.status } : { flagged: false };
  }
  // The previous reviewer wrote prose that names every file it read, so a mention
  // proves nothing. Its PRs are reported as undecided rather than guessed.
  if (record.review === "legacy") return { flagged: null, legacy: true };
  return { flagged: false, unreviewed: true };
}
