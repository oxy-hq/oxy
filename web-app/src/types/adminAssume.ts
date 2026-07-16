/**
 * An explicit Oxy-staff impersonation ("assume role") session.
 *
 * Staff have always been able to act as a tenant Owner — this makes that reach
 * opt-in, org-scoped, time-bounded, audited, and (via the banner) announced.
 */
export interface AssumeSession {
  id: string;
  org_id: string;
  org_name: string | null;
  /** The org's product surface is `/{slug}`. */
  org_slug: string | null;
  /**
   * The assumed org holds a partner grant, so the surface that matters is the
   * partner console — not an org dashboard.
   */
  is_partner: boolean;
  /** The REAL staff user — never the impersonated identity. */
  actor_email: string;
  reason: string;
  started_at: string;
  expires_at: string;
  expires_in_seconds: number;
}
