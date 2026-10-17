import type {
  NodeSettings,
  NotificationDevice,
  NotificationSettings,
  SettingsCategory,
  SettingsKind,
  SettingsSource
} from "@/types/notifications";

/** Sample settings documents for this section's tests, shaped as the server serves them. */

export const own = (over: Partial<NodeSettings> = {}): NodeSettings => ({
  channels: {},
  min_level: null,
  ...over
});

const delivered = { on: true, reason: null, from: null } as const;

export const kind = (
  over: Partial<SettingsKind> & { kind: string; label: string }
): SettingsKind => ({
  description: null,
  level: "active",
  critical: false,
  channels: ["push", "email", "slack"],
  settings: own(),
  effective: { push: delivered, email: delivered, slack: delivered },
  unread: 0,
  ...over
});

export const category = (
  over: Partial<SettingsCategory> & { path: string; label: string }
): SettingsCategory => ({
  description: null,
  settings: own(),
  categories: [],
  kinds: [],
  unread: 0,
  ...over
});

/**
 * Oxygen: billing (critical, push only) and runs (push and email).
 */
export const oxygen = (): SettingsSource => ({
  source: "oxygen",
  app: null,
  label: "Oxygen",
  settings: own(),
  unread: 0,
  categories: [
    category({
      path: "billing",
      label: "Billing",
      kinds: [
        kind({
          kind: "oxy.billing_past_due",
          label: "Payment past due",
          level: "critical",
          critical: true,
          channels: ["push"]
        })
      ]
    }),
    category({
      path: "runs",
      label: "Automations and pipelines",
      kinds: [kind({ kind: "oxy.run_failed", label: "Run failed", channels: ["push", "email"] })]
    })
  ]
});

/**
 * Store Ops: "Store operations" turns email off; "Food safety" under it turns
 * email back on; "Staffing" holds back anything below time-sensitive.
 */
export const storeOps = (): SettingsSource => ({
  source: "app",
  app: { id: "app-1", slug: "store-ops", name: "Store Ops", org_slug: "acme" },
  label: "Store Ops",
  settings: own(),
  unread: 0,
  categories: [
    category({
      path: "operations",
      label: "Store operations",
      settings: own({ channels: { email: false } }),
      kinds: [kind({ kind: "list_overdue", label: "Overdue lists", level: "time_sensitive" })],
      categories: [
        category({
          path: "operations/food-safety",
          label: "Food safety",
          description: "Temperature logs and checks",
          settings: own({ channels: { email: true } }),
          kinds: [
            kind({ kind: "temp_check", label: "Temperature check" }),
            kind({
              kind: "temp_alarm",
              label: "Temperature alarm",
              level: "critical",
              critical: true
            })
          ]
        })
      ]
    }),
    category({
      path: "staffing",
      label: "Staffing",
      settings: own({ min_level: "time_sensitive" }),
      kinds: [
        kind({ kind: "shift_swap", label: "Shift swap requests" }),
        kind({ kind: "swap_urgent", label: "Urgent swaps", level: "time_sensitive" })
      ]
    })
  ]
});

export const device = (over: Partial<NotificationDevice> & { id: string }): NotificationDevice => ({
  platform: "web",
  name: null,
  app_version: null,
  created_at: "2026-10-01T10:00:00Z",
  last_seen_at: "2026-10-17T09:00:00Z",
  enabled: true,
  min_level: null,
  os_permission: "granted",
  os_disabled: [],
  ...over
});

export const settingsDoc = (over: Partial<NotificationSettings> = {}): NotificationSettings => ({
  version: "v1",
  global: { paused_until: null, quiet_hours: null, timezone: null },
  sources: [storeOps(), oxygen()],
  devices: [],
  reach: { push_devices: 1, email: true, slack: true },
  ...over
});
