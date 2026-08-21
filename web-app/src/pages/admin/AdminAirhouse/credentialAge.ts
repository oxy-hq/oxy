/**
 * How to read a service account's rotation age.
 *
 * "Never rotated" is the phrase this exists for. On its own it reads
 * identically for a tenant provisioned this morning and one provisioned two
 * years ago — and only the second is a finding. An operator scanning for the
 * cause of a broken credential needs those two told apart at a glance, so the
 * age of the account is folded in rather than left for them to work out.
 *
 * Deliberately **not** a fourth `Severity`. Severity answers "can this tenant
 * serve a query right now", which a stale-but-working credential can; folding
 * age into it would make the filter chips claim a tenant is broken when it is
 * merely overdue, and the chips are the page's primary control.
 */
export const STALE_AFTER_DAYS = 90;

export interface CredentialAge {
  /** What the cell reads. */
  label: string;
  /** Whether it deserves the eye — overdue, or never rotated and not new. */
  overdue: boolean;
}

const DAY_MS = 86_400_000;
const HOUR_MS = 3_600_000;
const MINUTE_MS = 60_000;

/**
 * Absent and unreadable are different answers.
 *
 * Collapsing them into one `null` let a row whose rotation timestamp was
 * garbage fall through to the created-at branch and render `never · 400d old`
 * — asserting the credential was never rotated when in fact it was rotated at a
 * time we could not read. That is the exact class of false-but-reassuring claim
 * this module exists to prevent. Unreachable today (the API always emits
 * `to_rfc3339`), which is precisely when it is cheap to get right.
 */
type Elapsed = { kind: "absent" } | { kind: "unreadable" } | { kind: "ms"; ms: number };

function elapsed(iso: string | null | undefined, now: number): Elapsed {
  if (iso == null || iso === "") return { kind: "absent" };
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return { kind: "unreadable" };
  return { kind: "ms", ms: Math.max(0, now - t) };
}

/**
 * Coarse enough to scan, fine enough to confirm a rotation you just made.
 *
 * Days alone rendered a credential rotated three hours ago as `0d ago`, which
 * is the one moment an operator reloads this page specifically to check.
 */
function ago(ms: number): string {
  if (ms < MINUTE_MS) return "just now";
  if (ms < HOUR_MS) return `${Math.floor(ms / MINUTE_MS)}m ago`;
  if (ms < DAY_MS) return `${Math.floor(ms / HOUR_MS)}h ago`;
  return `${Math.floor(ms / DAY_MS)}d ago`;
}

export function credentialAge(
  row: { sa_rotated_at?: string | null; sa_created_at?: string | null },
  now: number = Date.now()
): CredentialAge {
  const rotated = elapsed(row.sa_rotated_at, now);
  if (rotated.kind === "unreadable") return { label: "unknown", overdue: false };
  if (rotated.kind === "ms") {
    return { label: ago(rotated.ms), overdue: rotated.ms >= STALE_AFTER_DAYS * DAY_MS };
  }

  const created = elapsed(row.sa_created_at, now);
  if (created.kind === "unreadable") return { label: "unknown", overdue: false };
  // No account bound at all: rotation age is not the question, and calling that
  // overdue would point the operator at the wrong fix — the account needs
  // binding, not rotating.
  if (created.kind === "absent") return { label: "—", overdue: false };
  const days = Math.floor(created.ms / DAY_MS);
  return { label: `never · ${days}d old`, overdue: created.ms >= STALE_AFTER_DAYS * DAY_MS };
}

/**
 * A credential lifetime, as an operator would say it.
 *
 * `== null` so an absent value from a field typed `number | null` — or an
 * `undefined` that slips past the type — reads as "—" rather than
 * `"undefineds"`.
 */
export function ttlLabel(seconds: number | null | undefined): string {
  if (seconds == null) return "—";
  // Zero is a real value and not an hour: `0 % 3600 === 0` would render "0h".
  if (seconds === 0) return "0s";
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}
