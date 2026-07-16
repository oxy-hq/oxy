import type {
  ChildOrg,
  CreatedOrg,
  MyPartner,
  PartnerApp,
  PartnerAuditEvent,
  PartnerCreatedToken,
  PartnerHealthRow,
  PartnerOrgMember,
  PartnerPerson,
  PartnerPublishToken,
  PartnerWorkspace
} from "@/types/partners";
import { apiClient } from "./axios";

/**
 * Partner console API (`/api/partners`) — a partner's own self-service surface.
 * Distinct from the Oxy-staff `/admin/partners` provisioning API.
 *
 * The server re-checks `role ∩ ceiling ∩ assignment` on every call, so two people
 * at the same partner get different answers from the same endpoints.
 */
export const PartnersService = {
  async listMine(): Promise<MyPartner[]> {
    const { data } = await apiClient.get("/partners");
    return data;
  },

  async orgs(partnerId: string): Promise<ChildOrg[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/orgs`);
    return data;
  },

  /**
   * Partner-initiated onboarding — requires `create_orgs`. Creating a client is
   * safe to delegate in a way attaching an existing org is not: a brand-new org
   * affects nobody else's tenant.
   */
  async createOrg(
    partnerId: string,
    body: { name: string; slug: string; owner_email?: string }
  ): Promise<CreatedOrg> {
    const { data } = await apiClient.post(`/partners/${partnerId}/orgs`, body);
    return data;
  },

  /** Rename a client org — requires `manage_org_settings`. The slug is the client's own call. */
  async updateOrg(partnerId: string, orgId: string, body: { name: string }): Promise<ChildOrg> {
    const { data } = await apiClient.patch(`/partners/${partnerId}/orgs/${orgId}`, body);
    return data;
  },

  async members(partnerId: string, orgId: string): Promise<PartnerOrgMember[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/orgs/${orgId}/members`);
    return data;
  },

  async orgApps(partnerId: string, orgId: string): Promise<PartnerApp[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/orgs/${orgId}/apps`);
    return data;
  },

  async workspaces(partnerId: string, orgId: string): Promise<PartnerWorkspace[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/orgs/${orgId}/workspaces`);
    return data;
  },

  async health(partnerId: string): Promise<PartnerHealthRow[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/health`);
    return data;
  },

  async appTokens(partnerId: string, appId: string): Promise<PartnerPublishToken[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/apps/${appId}/publish-tokens`);
    return data;
  },

  async createAppToken(
    partnerId: string,
    appId: string,
    name?: string
  ): Promise<PartnerCreatedToken> {
    const { data } = await apiClient.post(`/partners/${partnerId}/apps/${appId}/publish-tokens`, {
      name
    });
    return data;
  },

  async revokeAppToken(partnerId: string, appId: string, tokenId: string): Promise<void> {
    await apiClient.delete(`/partners/${partnerId}/apps/${appId}/publish-tokens/${tokenId}`);
  },

  async setAppPublished(partnerId: string, appId: string, published: boolean): Promise<PartnerApp> {
    const path = `/partners/${partnerId}/apps/${appId}/publish`;
    const { data } = published ? await apiClient.post(path) : await apiClient.delete(path);
    return data;
  },

  async audit(partnerId: string, limit = 200): Promise<PartnerAuditEvent[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/audit`, {
      params: { limit }
    });
    return data;
  },

  async inviteMember(
    partnerId: string,
    orgId: string,
    body: { email: string; role: string }
  ): Promise<void> {
    await apiClient.post(`/partners/${partnerId}/orgs/${orgId}/members`, body);
  },

  async updateMemberRole(
    partnerId: string,
    orgId: string,
    userId: string,
    role: string
  ): Promise<void> {
    await apiClient.patch(`/partners/${partnerId}/orgs/${orgId}/members/${userId}`, { role });
  },

  async removeMember(partnerId: string, orgId: string, userId: string): Promise<void> {
    await apiClient.delete(`/partners/${partnerId}/orgs/${orgId}/members/${userId}`);
  },

  // ── the partner staffing itself (partner_admin only) ──────────────────────

  async people(partnerId: string): Promise<PartnerPerson[]> {
    const { data } = await apiClient.get(`/partners/${partnerId}/people`);
    return data;
  },

  /** Grant partner access — this person becomes an operator over every client. */
  async grantAccess(partnerId: string, orgMemberId: string): Promise<PartnerPerson> {
    const { data } = await apiClient.put(`/partners/${partnerId}/people/${orgMemberId}`);
    return data;
  },

  /** Revoke partner access. They stay an employee — they just manage no clients. */
  async revokeAccess(partnerId: string, orgMemberId: string): Promise<void> {
    await apiClient.delete(`/partners/${partnerId}/people/${orgMemberId}`);
  }
};
