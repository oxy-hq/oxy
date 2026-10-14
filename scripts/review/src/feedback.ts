// What was said about findings: replies naming an id, thread replies, and reactions on
// inline comments. Bots are first-class here: an agent's or another bot's reply, rating
// or dispute counts exactly like a person's. Only the reviewer's own comments are left
// out, so it never grades itself.

import type { Finding, GhUser, IssueComment, ReviewComment } from "./types.ts";

const NEGATIVE =
  /\b(not useful|useless|false positive|wrong|noise|noisy|incorrect|disagree|irrelevant)\b/i;
const POSITIVE = /\b(useful|good catch|nice catch|valid|agreed|fixed it|thanks)\b/i;
const MENTION = /\bF(\d{1,3})\b/g;
const OWN_MARKER = "<!-- oxy-review:";

type Said = Pick<IssueComment, "user" | "body">;

export interface Feedback {
  id: string;
  kind: "useful" | "not_useful";
  note: string;
}

export interface Tally {
  useful: number;
  not_useful: number;
  notes: string[];
  replies: number;
}

const idsIn = (text: string): string[] => [...text.matchAll(MENTION)].map((m) => `F${m[1] ?? ""}`);

/** `F3 not useful: covered by the caller` → [{ id: "F3", kind: "not_useful", note }]. */
export function parseFeedback(text: string | null | undefined): Feedback[] {
  const out: Feedback[] = [];
  for (const line of (text ?? "").split("\n")) {
    if (line.trim().startsWith(">")) continue; // quoted text is someone else's words
    const ids = idsIn(line);
    if (!ids.length) continue;
    const kind = NEGATIVE.test(line) ? "not_useful" : POSITIVE.test(line) ? "useful" : null;
    if (!kind) continue;
    const note = line
      .replace(MENTION, "")
      .replace(/^[\s:,.-]+/, "")
      .trim()
      .slice(0, 300);
    for (const id of new Set(ids)) out.push({ id, kind, note });
  }
  return out;
}

export function isBot(user: GhUser | null, botLogins: readonly string[]): boolean {
  if (!user) return true;
  if (user.type === "Bot" || /\[bot\]$/.test(user.login)) return true;
  return botLogins.includes(user.login.replace(/\[bot\]$/, ""));
}

/** A comment the reviewer itself posted: its identity, carrying its marker. */
export function isOwn(comment: Said, reviewerLogins: readonly string[]): boolean {
  const login = (comment.user?.login ?? "").replace(/\[bot\]$/, "");
  return reviewerLogins.includes(login) && (comment.body ?? "").includes(OWN_MARKER);
}

/** Who said it, for a prompt: `name` or `name (bot)`. */
export const speaker = (c: Said): string =>
  `${c.user?.login ?? "unknown"}${c.user?.type === "Bot" ? " (bot)" : ""}`;

/**
 * Per-finding feedback from PR conversation comments, inline-thread replies and
 * reactions on the inline comment that carries each finding.
 */
export function collectFeedback({
  issueComments,
  reviewComments,
  findings,
  reviewerLogins
}: {
  issueComments: readonly IssueComment[];
  reviewComments: readonly ReviewComment[];
  findings: readonly Pick<Finding, "id" | "inline">[];
  reviewerLogins: readonly string[];
}): Map<string, Tally> {
  const tally = new Map<string, Tally>(
    findings.map((f) => [f.id, { useful: 0, not_useful: 0, notes: [], replies: 0 }])
  );
  const others = (c: Said) => !isOwn(c, reviewerLogins);
  for (const c of [...issueComments, ...reviewComments].filter(others)) {
    for (const fb of parseFeedback(c.body)) {
      const t = tally.get(fb.id);
      if (!t) continue;
      t[fb.kind]++;
      if (fb.note) t.notes.push(fb.note);
    }
  }
  const byInline = new Map(findings.filter((f) => f.inline).map((f) => [f.inline, f.id]));
  for (const c of reviewComments) {
    const own = tally.get(byInline.get(c.id) ?? "");
    if (own) {
      own.useful += c.reactions?.["+1"] ?? 0;
      own.not_useful += c.reactions?.["-1"] ?? 0;
    }
    const parent = tally.get(byInline.get(c.in_reply_to_id ?? null) ?? "");
    if (parent && others(c)) parent.replies++;
  }
  return tally;
}

/**
 * What anyone but the reviewer said about each finding, for the recheck pass to weigh:
 * replies in its inline thread, and PR comments that name its id.
 */
export function threadReplies({
  reviewComments,
  issueComments = [],
  findings,
  reviewerLogins
}: {
  reviewComments: readonly ReviewComment[];
  issueComments?: readonly IssueComment[];
  findings: readonly Pick<Finding, "id" | "inline">[];
  reviewerLogins: readonly string[];
}): Map<string, string[]> {
  const byInline = new Map(findings.filter((f) => f.inline).map((f) => [f.inline, f.id]));
  const ids = new Set(findings.map((f) => f.id));
  const out = new Map<string, string[]>();
  const add = (id: string, c: Said) => {
    const list = out.get(id) ?? [];
    list.push(`${speaker(c)}: ${(c.body ?? "").slice(0, 1200)}`);
    out.set(id, list);
  };
  for (const c of reviewComments) {
    const id = byInline.get(c.in_reply_to_id ?? null);
    if (id && !isOwn(c, reviewerLogins)) add(id, c);
  }
  for (const c of issueComments) {
    if (isOwn(c, reviewerLogins)) continue;
    for (const id of new Set(idsIn(c.body ?? ""))) if (ids.has(id)) add(id, c);
  }
  return out;
}

/** The PR's conversation, newest last, for the reviewer to read as context. */
export function discussion(
  issueComments: readonly IssueComment[],
  reviewerLogins: readonly string[],
  { max = 8, chars = 600 }: { max?: number; chars?: number } = {}
): string[] {
  return issueComments
    .filter((c) => !isOwn(c, reviewerLogins))
    .slice(-max)
    .map(
      (c) =>
        `${speaker(c)}: ${(c.body ?? "")
          .replace(/<!--[\s\S]*?-->/g, "")
          .trim()
          .slice(0, chars)}`
    )
    .filter((line) => !line.endsWith(": "));
}
