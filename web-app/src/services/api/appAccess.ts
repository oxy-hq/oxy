import type {
  AppAccess,
  AppAccessSummary,
  GrantablePerson,
  SetAppAccessRequest,
  Team,
  TeamDetail
} from "@/types/appAccess";
import { apiClient } from "./axios";

/**
 * Org teams — the audiences an org grants apps to.
 *
 * Gated server-side by `Action::AppAccessManage`: an org owner/admin, Oxy staff,
 * or a partner holding `manage_apps`.
 */
export class TeamService {
  static async list(orgId: string): Promise<Team[]> {
    const response = await apiClient.get<Team[]>(`/orgs/${orgId}/teams`);
    return response.data;
  }

  static async get(orgId: string, teamId: string): Promise<TeamDetail> {
    const response = await apiClient.get<TeamDetail>(`/orgs/${orgId}/teams/${teamId}`);
    return response.data;
  }

  static async create(
    orgId: string,
    data: { name: string; description?: string | null }
  ): Promise<Team> {
    const response = await apiClient.post<Team>(`/orgs/${orgId}/teams`, data);
    return response.data;
  }

  static async update(
    orgId: string,
    teamId: string,
    data: { name: string; description?: string | null }
  ): Promise<Team> {
    const response = await apiClient.patch<Team>(`/orgs/${orgId}/teams/${teamId}`, data);
    return response.data;
  }

  static async remove(orgId: string, teamId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/teams/${teamId}`);
  }

  static async addMember(orgId: string, teamId: string, userId: string): Promise<void> {
    await apiClient.post(`/orgs/${orgId}/teams/${teamId}/members`, { user_id: userId });
  }

  static async removeMember(orgId: string, teamId: string, userId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/teams/${teamId}/members/${userId}`);
  }
}

/**
 * Who may open a custom app.
 *
 * Three surfaces edit the same data behind three different gates, because they
 * authenticate in ways that can't be unified: org routes need a real membership or
 * a live assume-role session, `/admin/*` is closed while an operator is acting, and
 * the partner console is capability-scoped. Same request and response shape on all
 * three, so the UI is one component.
 */
export class AppAccessService {
  /**
   * The org's apps with their visibility — NOT the launcher's list, which is
   * filtered to what the viewer can open. An admin managing access needs to see
   * the apps they can't personally open too.
   */
  static async listOrgApps(orgId: string): Promise<AppAccessSummary[]> {
    const response = await apiClient.get<AppAccessSummary[]>(`/orgs/${orgId}/apps`);
    return response.data;
  }

  static async getForOrg(orgId: string, appId: string): Promise<AppAccess> {
    const response = await apiClient.get<AppAccess>(`/orgs/${orgId}/apps/${appId}/access`);
    return response.data;
  }

  static async setForOrg(
    orgId: string,
    appId: string,
    body: SetAppAccessRequest
  ): Promise<AppAccess> {
    const response = await apiClient.put<AppAccess>(`/orgs/${orgId}/apps/${appId}/access`, body);
    return response.data;
  }

  // ── Oxy admin console. Keyed by app alone; the server derives the org. ──

  static async getForAdmin(appId: string): Promise<AppAccess> {
    const response = await apiClient.get<AppAccess>(`/admin/apps/${appId}/access`);
    return response.data;
  }

  static async setForAdmin(appId: string, body: SetAppAccessRequest): Promise<AppAccess> {
    const response = await apiClient.put<AppAccess>(`/admin/apps/${appId}/access`, body);
    return response.data;
  }

  static async listTeamsForAdmin(appId: string): Promise<Team[]> {
    const response = await apiClient.get<Team[]>(`/admin/apps/${appId}/teams`);
    return response.data;
  }

  /**
   * The owning org's people. Keyed by app rather than org because the console
   * never holds an org id — the server derives the tenant from the app row.
   */
  static async listMembersForAdmin(appId: string): Promise<GrantablePerson[]> {
    const response = await apiClient.get<GrantablePerson[]>(`/admin/apps/${appId}/members`);
    return response.data;
  }

  // ── Partner console. Scoped by the partner being acted as. ──

  static async getForPartner(partnerId: string, appId: string): Promise<AppAccess> {
    const response = await apiClient.get<AppAccess>(`/partners/${partnerId}/apps/${appId}/access`);
    return response.data;
  }

  static async setForPartner(
    partnerId: string,
    appId: string,
    body: SetAppAccessRequest
  ): Promise<AppAccess> {
    const response = await apiClient.put<AppAccess>(
      `/partners/${partnerId}/apps/${appId}/access`,
      body
    );
    return response.data;
  }

  static async listTeamsForPartner(partnerId: string, orgId: string): Promise<Team[]> {
    const response = await apiClient.get<Team[]>(`/partners/${partnerId}/orgs/${orgId}/teams`);
    return response.data;
  }

  /**
   * The client org's people, gated on `manage_apps`.
   *
   * Deliberately NOT `/partners/{id}/orgs/{org}/members`, which requires
   * `manage_members` — a different capability that a partner managing apps for a
   * client is not expected to hold. Routing the picker through that endpoint gave
   * such a partner a silent 403: the People group vanished, and any existing
   * individual grants rendered as "Unknown person" and could be saved back under
   * that label.
   */
  static async listPeopleForPartner(partnerId: string, orgId: string): Promise<GrantablePerson[]> {
    const response = await apiClient.get<GrantablePerson[]>(
      `/partners/${partnerId}/orgs/${orgId}/grantable-people`
    );
    return response.data;
  }

  // ── The partner's OWN org. A partner is a real org with its own apps, and it is
  // not one of its own clients — so these are separate routes, authorized by org
  // authority (an officer of the partner org) rather than the partner ceiling.

  static async listOwnApps(partnerId: string): Promise<AppAccessSummary[]> {
    const response = await apiClient.get<AppAccessSummary[]>(`/partners/${partnerId}/own-apps`);
    return response.data;
  }

  static async listOwnTeams(partnerId: string): Promise<Team[]> {
    const response = await apiClient.get<Team[]>(`/partners/${partnerId}/own-teams`);
    return response.data;
  }

  static async listOwnPeople(partnerId: string): Promise<GrantablePerson[]> {
    const response = await apiClient.get<GrantablePerson[]>(`/partners/${partnerId}/own-people`);
    return response.data;
  }
}
