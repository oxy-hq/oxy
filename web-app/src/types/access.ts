/**
 * Global "Oxy app admin" role. Members have access to the customer-apps
 * surface and every registered customer app regardless of org membership.
 * Managed by `OXY_OWNER` users from `/admin/app-admins`.
 */
export interface AppAdmin {
  id: string;
  email: string;
  /** User who added this admin. Null for env-bootstrapped rows. */
  granted_by: string | null;
  created_at: string;
}

/**
 * Status of the per-workspace "Oxy can build tailored apps on our
 * data" toggle. When `enabled` is true, anyone in the `app_admins`
 * table can access customer apps in this workspace.
 *
 * Audit fields are only populated when `enabled` is true.
 */
export interface OxyAccessStatus {
  enabled: boolean;
  granted_by: string | null;
  granted_at: string | null;
}
