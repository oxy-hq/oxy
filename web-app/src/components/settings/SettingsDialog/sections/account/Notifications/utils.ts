import type { NotificationChannel } from "@/types/notifications";

/** The channels a person can mute, in the order the switches show them. */
export const CHANNELS: { value: NotificationChannel; label: string }[] = [
  { value: "push", label: "Push" },
  { value: "email", label: "Email" },
  { value: "slack", label: "Slack" }
];

/** A kind as the app named it, for reading: `list_overdue` → "List overdue". */
export function humanizeKind(kind: string): string {
  const words = kind.replace(/[_.-]+/g, " ").trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : kind;
}

/** Oxygen's own kinds, as people read them. */
const PLATFORM_KIND_LABELS: Partial<Record<string, string>> = {
  "oxy.billing_past_due": "Billing past due",
  "oxy.workspace_ready": "Workspace ready",
  "oxy.run_failed": "Run failed",
  "oxy.anomaly": "Anomalies"
};

/** The prefix every Oxygen kind carries, and no app may use. */
const PLATFORM_PREFIX = "oxy.";

/** A kind's label: Oxygen's own by name, anything else (a new `oxy.*` kind too) humanized. */
export function kindLabel(kind: string): string {
  const known = PLATFORM_KIND_LABELS[kind];
  if (known) return known;
  return humanizeKind(kind.startsWith(PLATFORM_PREFIX) ? kind.slice(PLATFORM_PREFIX.length) : kind);
}

/** How many devices push reaches, in a few words. */
export function deviceCount(n: number): string {
  if (n <= 0) return "Not on any device yet";
  return n === 1 ? "Reaches 1 device" : `Reaches ${n} devices`;
}
