import type { AdminStatusTone } from "@/pages/admin/components/AdminStatusPill";
import type { AppAvailability } from "@/types/apps";

/**
 * Verdict → status-pill tone, mapped onto the existing admin enumeration rather
 * than inventing a sixth tone (see `AdminStatusPill`).
 *
 * `no_opinion` is **muted, not ok**. An app nobody has used has not been shown
 * to work, and rendering silence as a green tick is exactly how a dead app
 * reassures the operator looking at it. `burning` splits by severity because the
 * two grades have different consequences: `page` reaches on-call, `ticket` never
 * does — the same split `custom_apps::status_for` makes on the backend when it
 * maps them onto Unhealthy vs Degraded.
 */
export function availabilityTone(data: AppAvailability): AdminStatusTone {
  switch (data.verdict) {
    case "healthy":
      return "ok";
    case "burning":
      return data.severity === "page" ? "danger" : "warn";
    case "no_opinion":
      return "muted";
  }
}

/** Short label for the pill. */
export function availabilityLabel(data: AppAvailability): string {
  switch (data.verdict) {
    case "healthy":
      return "serving";
    case "burning":
      return data.severity === "page" ? "burning" : "degraded";
    case "no_opinion":
      return "no data";
  }
}

/** `5` → `5m`, `1440` → `24h`. */
export function formatWindow(minutes: number): string {
  return minutes >= 60 && minutes % 60 === 0 ? `${minutes / 60}h` : `${minutes}m`;
}

/** `0.99` → `99%`, `0.995` → `99.5%`. */
export function formatObjective(objective: number): string {
  const pct = objective * 100;
  return `${pct.toFixed(pct % 1 === 0 ? 0 : 1)}%`;
}
