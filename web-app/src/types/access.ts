/**
 * Global "Oxy app admin" role. Members have access to the customer-apps
 * surface and every registered custom app regardless of org membership.
 * Managed by `OXY_OWNER` users from `/admin/app-admins`.
 */
import type { PlatformCapability } from "./auth";

/**
 * A **platform grant** — one person's Oxy-staff standing as `(role × scope)`.
 *
 * The type name predates the capability split, when every row meant the same thing.
 * It now carries the role the grant was issued as, so an App Operator and a Global
 * Admin are different rows rather than indistinguishable ones.
 */
export interface AppAdmin {
  id: string;
  email: string;
  /** Who last set this grant to what it is. Null for env-bootstrapped rows. */
  granted_by: string | null;
  created_at: string;
  /**
   * When the grant last changed. A grant is upserted in place, so `created_at` answers
   * "when did they first get access" and this answers "when did it become what it is" —
   * equal for one that has never changed.
   */
  updated_at: string;
  /** `global_admin` or `app_operator`. */
  role: PlatformRoleId;
  /** True = every org, present and future. False = the orgs in `scope_org_ids`. */
  scope_all: boolean;
  /** Empty when `scope_all` is true. */
  scope_org_ids: string[];
  /** What `role` expands to, derived server-side so the UI never re-implements it. */
  capabilities: PlatformCapability[];
}

export type PlatformRoleId = "global_admin" | "app_operator";

export interface CreateAppAdminInput {
  email: string;
  role: PlatformRoleId;
  /** Omit for an unbounded grant; a list bounds it to exactly those orgs. */
  scope_org_ids?: string[];
}

/**
 * Status of the per-workspace "Oxy can build tailored apps on our
 * data" toggle. When `enabled` is true, anyone in the `app_admins`
 * table can access custom apps in this workspace.
 *
 * Audit fields are only populated when `enabled` is true.
 */
export interface OxyAccessStatus {
  /** True when the org has LOCKED Oxy staff OUT. Default is false (access allowed). */
  locked: boolean;
  locked_by: string | null;
  locked_at: string | null;
  /**
   * Whether THIS caller may flip the switch. Only a real org owner/admin can —
   * an Oxy operator viewing the workspace sees the state but cannot change it
   * (they must not be able to unlock themselves).
   */
  can_manage: boolean;
}

/** Read-only org-subdomain status shown in customer settings. */
export interface OrgSubdomainStatus {
  enabled: boolean;
  /** The org slug — this is the subdomain label. */
  subdomain: string;
  /** Full URL `https://<slug>.<zone>/`, or null (disabled / zone not derivable). */
  url: string | null;
  /** True when the current workspace is the subdomain's default project. */
  is_default_workspace: boolean;
}
