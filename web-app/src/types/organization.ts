export type OrgRole = "owner" | "admin" | "member";
export type WorkspaceRole = "owner" | "admin" | "member" | "viewer";

export interface Organization {
  id: string;
  name: string;
  slug: string;
  role: OrgRole;
  created_at?: string;
  workspace_count?: number;
  member_count?: number;
}

export interface OrgMember {
  id: string;
  user_id: string;
  email: string;
  name: string;
  role: OrgRole;
  created_at: string;
}

export interface WorkspaceMember {
  user_id: string;
  email: string;
  name: string;
  org_role: OrgRole;
  workspace_role: WorkspaceRole;
  is_override: boolean;
}

export interface OrgInvitation {
  id: string;
  email: string;
  role: OrgRole;
  token: string;
  status: "pending" | "accepted" | "expired";
  expires_at: string;
  created_at: string;
}

export interface MyInvitation {
  id: string;
  token: string;
  role: OrgRole;
  expires_at: string;
  created_at: string;
  org_id: string;
  org_name: string;
  org_slug: string;
  invited_by_name?: string;
}
