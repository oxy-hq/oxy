import { apiClient } from "./axios";

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

export type WorkspaceStatusId = "ready" | "cloning" | "failed" | "not_oxy_project";
export type UserStatusId = "active" | "deleted";
export type OrgRoleId = "owner" | "admin" | "member";

// ---------------------------------------------------------------------------
// Organizations (admin meta surface)
// ---------------------------------------------------------------------------

export interface AdminOrgMeta {
  id: string;
  name: string;
  slug: string;
  created_at: string;
  member_count: number;
  workspace_count: number;
  owner_email: string | null;
}

export interface AdminOrgDetail extends AdminOrgMeta {
  updated_at: string;
  owners: AdminOrgUserSummary[];
  workspaces: AdminOrgWorkspaceSummary[];
}

export interface AdminOrgUserSummary {
  user_id: string;
  email: string;
  name: string;
  role: string;
}

export interface AdminOrgWorkspaceSummary {
  id: string;
  name: string;
  status: WorkspaceStatusId;
  created_at: string;
}

export interface ListOrgsMetaQuery {
  search?: string;
  page?: number;
  page_size?: number;
}

export interface RenameOrgBody {
  name?: string;
  slug?: string;
}

export const AdminOrgsService = {
  async list(query: ListOrgsMetaQuery = {}): Promise<AdminOrgMeta[]> {
    const res = await apiClient.get<AdminOrgMeta[]>("/admin/orgs-meta", { params: query });
    return res.data;
  },

  async detail(orgId: string): Promise<AdminOrgDetail> {
    const res = await apiClient.get<AdminOrgDetail>(`/admin/orgs/${orgId}/detail`);
    return res.data;
  },

  async rename(orgId: string, body: RenameOrgBody): Promise<AdminOrgMeta> {
    const res = await apiClient.patch<AdminOrgMeta>(`/admin/orgs/${orgId}`, body);
    return res.data;
  },

  async remove(orgId: string): Promise<void> {
    await apiClient.delete(`/admin/orgs/${orgId}`);
  },

  async transferOwnership(orgId: string, newOwnerUserId: string): Promise<void> {
    await apiClient.post(`/admin/orgs/${orgId}/transfer-ownership`, {
      new_owner_user_id: newOwnerUserId
    });
  }
};

// ---------------------------------------------------------------------------
// Users (admin meta surface)
// ---------------------------------------------------------------------------

export interface AdminUserRow {
  id: string;
  email: string;
  name: string;
  status: UserStatusId;
  created_at: string;
  last_login_at: string;
  is_app_admin: boolean;
  org_count: number;
}

export interface AdminUserDetail extends AdminUserRow {
  picture: string | null;
  email_verified: boolean;
  org_memberships: AdminUserOrgMembership[];
  workspace_memberships: AdminUserWorkspaceMembership[];
}

export interface AdminUserOrgMembership {
  org_id: string;
  org_slug: string;
  org_name: string;
  role: string;
  joined_at: string;
}

export interface AdminUserWorkspaceMembership {
  workspace_id: string;
  workspace_name: string;
  role: string;
  joined_at: string;
}

export interface ListUsersQuery {
  search?: string;
  status?: UserStatusId;
  page?: number;
  page_size?: number;
}

export const AdminUsersService = {
  async list(query: ListUsersQuery = {}): Promise<AdminUserRow[]> {
    const res = await apiClient.get<AdminUserRow[]>("/admin/users", { params: query });
    return res.data;
  },

  async detail(userId: string): Promise<AdminUserDetail> {
    const res = await apiClient.get<AdminUserDetail>(`/admin/users/${userId}`);
    return res.data;
  },

  async setStatus(userId: string, status: UserStatusId): Promise<void> {
    await apiClient.patch(`/admin/users/${userId}/status`, { status });
  },

  async addToOrg(userId: string, orgId: string, role: OrgRoleId): Promise<void> {
    await apiClient.post(`/admin/users/${userId}/org-memberships`, { org_id: orgId, role });
  },

  async updateRole(userId: string, orgId: string, role: OrgRoleId): Promise<void> {
    await apiClient.patch(`/admin/users/${userId}/org-memberships/${orgId}`, { role });
  },

  async removeFromOrg(userId: string, orgId: string): Promise<void> {
    await apiClient.delete(`/admin/users/${userId}/org-memberships/${orgId}`);
  }
};

// ---------------------------------------------------------------------------
// Workspaces (admin meta surface)
// ---------------------------------------------------------------------------

export interface AdminWorkspaceRow {
  id: string;
  name: string;
  status: WorkspaceStatusId;
  created_at: string;
  last_opened_at: string | null;
  org_id: string | null;
  org_slug: string | null;
  member_count: number;
}

export interface AdminWorkspaceDetail extends AdminWorkspaceRow {
  updated_at: string;
  org_name: string | null;
  path: string | null;
  git_remote_url: string | null;
  error: string | null;
  members: AdminWorkspaceMember[];
}

export interface AdminWorkspaceMember {
  user_id: string;
  email: string;
  name: string;
  role: string;
  joined_at: string;
}

export interface ListWorkspacesQuery {
  search?: string;
  status?: WorkspaceStatusId;
  org_id?: string;
  page?: number;
  page_size?: number;
}

export const AdminWorkspacesService = {
  async list(query: ListWorkspacesQuery = {}): Promise<AdminWorkspaceRow[]> {
    const res = await apiClient.get<AdminWorkspaceRow[]>("/admin/workspaces-meta", {
      params: query
    });
    return res.data;
  },

  async detail(workspaceId: string): Promise<AdminWorkspaceDetail> {
    const res = await apiClient.get<AdminWorkspaceDetail>(
      `/admin/workspaces/${workspaceId}/detail`
    );
    return res.data;
  },

  async rename(workspaceId: string, name: string): Promise<void> {
    await apiClient.patch(`/admin/workspaces/${workspaceId}`, { name });
  },

  async remove(workspaceId: string): Promise<void> {
    await apiClient.delete(`/admin/workspaces/${workspaceId}`);
  },

  async transferOrg(workspaceId: string, newOrgId: string): Promise<void> {
    await apiClient.post(`/admin/workspaces/${workspaceId}/transfer-org`, {
      new_org_id: newOrgId
    });
  }
};
