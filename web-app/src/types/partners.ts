import type { PartnerCapabilities } from "@/types/auth";

/** A partner the current user operates, from `GET /partners`. */
export interface MyPartner {
  /** The partner IS an org, so this is an org id. */
  partner_id: string;
  slug: string;
  name: string;
  org_count: number;
  /** The partner's ceiling — what any operator here can do. */
  capabilities: PartnerCapabilities;
}

/** A client org the partner manages, with headline counts. */
export interface ChildOrg {
  org_id: string;
  name: string;
  slug: string;
  member_count: number;
  app_count: number;
}

/** Result of partner-initiated onboarding (`POST /partners/:id/orgs`). */
export interface CreatedOrg {
  org: ChildOrg;
  /** How the first owner was onboarded: `seeded` (existing user added as Owner),
   *  `invited` (unknown email emailed an Owner invite), or `none` (no email given). */
  owner_status: "seeded" | "invited" | "none";
}

/** A workspace in a client org, from `GET /partners/:id/orgs/:orgId/workspaces`. */
export interface PartnerWorkspace {
  id: string;
  name: string;
  /** `ready` | `preparing` | `error` */
  status: string;
  /** A workspace with no compiled revision opens empty. */
  has_revision: boolean;
  last_opened_at: string | null;
  updated_at: string;
  error: string | null;
}

/** One workspace's health, from `GET /partners/:id/health` (worst-first). */
export interface PartnerHealthRow {
  workspace_id: string;
  workspace_name: string | null;
  org_name: string | null;
  /** `healthy` | `degraded` | `unhealthy` (and possibly `unknown`). */
  status: string;
  /** Why it's degraded/unhealthy — human-readable lines. */
  reasons: string[];
  /** When the periodic sweep last evaluated it. `null` until first sweep. */
  checked_at: string | null;
}

/** An app-scoped publish token, from `GET /partners/:id/apps/:appId/publish-tokens`. */
export interface PartnerPublishToken {
  id: string;
  name: string;
  token_prefix: string;
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
}

/** A freshly minted token — plaintext shown once. */
export interface PartnerCreatedToken {
  id: string;
  token: string;
  name: string;
  token_prefix: string;
  created_at: string;
  expires_at: string;
}

/**
 * The narrow shape publish/unpublish return (`PartnerAppDto`).
 *
 * Deliberately NOT widened with visibility/grant_count: the LIST route returns the
 * access-bearing `AppAccessSummary`, but the publish handlers still return these
 * four fields — declaring the extra keys here would have TypeScript assert them on
 * a payload that doesn't carry them.
 */
export interface PartnerApp {
  id: string;
  slug: string;
  name: string;
  published: boolean;
}

/** A member of a client org. */
export interface PartnerOrgMember {
  user_id: string;
  email: string;
  name: string | null;
  role: string;
}

/**
 * One of the partner's OWN people, from `GET /partners/:id/people`. This is the
 * partner staffing itself — `has_access: false` means an ordinary employee who
 * manages no clients.
 */
export interface PartnerPerson {
  org_member_id: string;
  user_id: string;
  email: string;
  name: string | null;
  /** Their role in the partner org itself (owner/admin/member). */
  org_role: string;
  /** Whether they are a partner operator. */
  has_access: boolean;
}

/** An audit event in the partner subtree, from `GET /partners/:id/audit`. */
export interface PartnerAuditEvent {
  id: string;
  created_at: string;
  actor_email: string;
  action: string;
  org_id: string | null;
  target_label: string | null;
  outcome: string;
}
