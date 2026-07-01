import type { WorkspaceHealthStatus } from "@/services/api/workspaceHealth";
import type { AdminStatusTone } from "./AdminStatusPill";

/**
 * Maps a workspace health status to a semantic status-pill tone. Shared
 * by the cross-tenant rollup (`AdminWorkspaceHealth`) and the per-workspace
 * Health tab on the workspace detail page.
 *
 * Tones match the rest of the admin operator console (where `ok` already
 * renders emerald for e.g. a "Ready" workspace) — the customer-facing
 * "emerald = workflow-node success only" rule does not apply to this surface.
 *   unhealthy → danger  (destructive token)
 *   degraded  → warn    (amber token)
 *   healthy   → ok      (emerald token)
 */
export function workspaceHealthTone(status: WorkspaceHealthStatus): AdminStatusTone {
  switch (status) {
    case "unhealthy":
      return "danger";
    case "degraded":
      return "warn";
    case "healthy":
      return "ok";
  }
}
