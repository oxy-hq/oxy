import type { CompileStatus } from "@/services/api/compile";

export type ViewKind =
  | "never"
  | "fresh"
  /** Serving revision contains everything on origin, plus local commits not yet pushed. */
  | "ahead"
  | "stale"
  | "unverified"
  | "compiling"
  | "failed"
  | "no-git";

export interface View {
  kind: ViewKind;
  verb: string;
  /** Short SHA chip text, e.g. `a3f4d12` or `a3f4d12 ↑`. Empty = hide chip. */
  sha: string | null;
}

/**
 * Beyond this age the last fetch is too old to make a claim from: origin may
 * have moved and this clone would not know. Ten minutes is comfortably longer
 * than the background fetch interval, so a healthy workspace never trips it.
 */
const REMOTE_STALE_AFTER_MS = 10 * 60 * 1000;

/**
 * The badge answers one question — *is what I merged actually live?* — so it
 * compares the two SHAs that question is about:
 *
 *   compiled_sha  what the runtime serves   (promoted revision)
 *   remote_sha    what was merged           (origin/<default>)
 *
 * It deliberately does NOT compare against `head_sha`. The working copy is the
 * input to a compile, not evidence about it: since compiles are taken from
 * `head_sha`, comparing a revision back against `head_sha` is circular and
 * reports "Up to date" for any workspace whose local HEAD has not moved —
 * including one sitting many commits behind origin. That is exactly the false
 * green badge in oxygen-workspace-sync-bugs.md bug 3.
 *
 * When the remote tip is unknown or the fetch is stale, the honest answer is
 * "unverified", not "up to date". A badge that cannot know must not assert.
 */
export function deriveView(status: CompileStatus | undefined): View {
  if (!status) {
    return { kind: "never", verb: "Compile", sha: null };
  }

  const latest = status.latest;
  if (latest?.status === "compiling") {
    return {
      kind: "compiling",
      verb: "Compiling…",
      sha: latest.git_sha ? short(latest.git_sha) : null
    };
  }
  if (latest?.status === "failed") {
    return {
      kind: "failed",
      verb: "Retry compile",
      sha: latest.git_sha ? short(latest.git_sha) : null
    };
  }

  // Blank / demo / no-remote — no SHA to track freshness against.
  if (!status.head_sha) {
    return { kind: "no-git", verb: "Compile", sha: null };
  }

  // Nothing promoted yet ⇒ nothing is being served from a revision at all.
  if (!status.compiled_sha) {
    return { kind: "never", verb: "Compile", sha: short(status.head_sha) };
  }

  // Remote tip unknown, or last fetch too old to trust. Show what IS known
  // (the serving revision) and say plainly that it hasn't been verified
  // against origin — rather than implying it has.
  if (!status.remote_sha || isRemoteStale(status.remote_fetched_at)) {
    return {
      kind: "unverified",
      verb: "Compiled",
      sha: short(status.compiled_sha)
    };
  }

  if (status.compiled_sha === status.remote_sha) {
    return { kind: "fresh", verb: "Up to date", sha: short(status.compiled_sha) };
  }

  // Differing SHAs are NOT automatically "behind". A revision compiled from a
  // local-only commit — every restore mints one, and restore auto-compiles —
  // is *ahead* of origin and fails the same equality. Treating that as stale
  // would tell the operator to compile toward an older origin SHA. Ancestry
  // comes from the server; equality alone can't tell the two apart.
  if (status.compiled_behind === 0 && (status.compiled_ahead ?? 0) > 0) {
    return { kind: "ahead", verb: "Up to date", sha: short(status.compiled_sha) };
  }

  // Origin has moved past what is being served — the actionable state. Also
  // the fallback when ancestry is unavailable (`compiled_behind === null`):
  // "there may be something unshipped" is the safe direction to err, since it
  // prompts a compile rather than asserting everything is live.
  return { kind: "stale", verb: "Compile", sha: `${short(status.remote_sha)} ↑` };
}

function isRemoteStale(fetchedAt: string | null): boolean {
  if (!fetchedAt) return true;
  const ts = new Date(fetchedAt).getTime();
  if (Number.isNaN(ts)) return true;
  return Date.now() - ts > REMOTE_STALE_AFTER_MS;
}

export function short(sha: string): string {
  return sha.length > 7 ? sha.slice(0, 7) : sha;
}
