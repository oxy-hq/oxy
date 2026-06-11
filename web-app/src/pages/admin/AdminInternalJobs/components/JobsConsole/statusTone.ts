/**
 * Cockpit status palette. Status is carried by a small left-border accent +
 * glyph rather than a big badge, so these return the Tailwind classes the
 * row/debug panel apply. Tones reuse the admin surface's established
 * emerald / amber / destructive convention.
 */
export interface StatusTone {
  /** Dot / left-accent background. */
  accent: string;
  /** Text color for the status label. */
  text: string;
}

export function statusTone(status: string): StatusTone {
  switch (status) {
    case "dead":
      return { accent: "bg-destructive", text: "text-destructive" };
    case "failed":
      return { accent: "bg-amber-500", text: "text-amber-700 dark:text-amber-400" };
    case "claimed":
      return { accent: "bg-primary", text: "text-primary" };
    case "completed":
      return {
        accent: "bg-emerald-500",
        text: "text-emerald-700 dark:text-emerald-400"
      };
    case "cancelled":
      return { accent: "bg-muted-foreground/60", text: "text-muted-foreground" };
    default:
      return { accent: "bg-foreground/60", text: "text-foreground" };
  }
}
