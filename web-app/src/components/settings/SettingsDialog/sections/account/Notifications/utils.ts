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

/** How many devices push reaches, in a few words. */
export function deviceCount(n: number): string {
  if (n <= 0) return "Not on any device yet";
  return n === 1 ? "Reaches 1 device" : `Reaches ${n} devices`;
}
