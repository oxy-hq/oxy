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
