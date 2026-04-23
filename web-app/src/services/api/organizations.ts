import type { MyInvitation, Organization, OrgInvitation, OrgMember } from "@/types/organization";
import { apiClient } from "./axios";

export class OrganizationService {
  static async createOrg(data: { name: string; slug: string }): Promise<Organization> {
    const response = await apiClient.post<Organization>("/orgs", data);
    return response.data;
  }

  static async listOrgs(): Promise<Organization[]> {
    const response = await apiClient.get<Organization[]>("/orgs");
    return response.data;
  }

  static async getOrg(orgId: string): Promise<Organization> {
    const response = await apiClient.get<Organization>(`/orgs/${orgId}`);
    return response.data;
  }

  static async updateOrg(
    orgId: string,
    data: { name?: string; slug?: string }
  ): Promise<Organization> {
    const response = await apiClient.patch<Organization>(`/orgs/${orgId}`, data);
    return response.data;
  }

  static async deleteOrg(orgId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}`);
  }

  static async listMembers(orgId: string): Promise<OrgMember[]> {
    const response = await apiClient.get<OrgMember[]>(`/orgs/${orgId}/members`);
    return response.data;
  }

  static async updateMemberRole(orgId: string, userId: string, role: string): Promise<OrgMember> {
    const response = await apiClient.patch<OrgMember>(`/orgs/${orgId}/members/${userId}`, { role });
    return response.data;
  }

  static async removeMember(orgId: string, userId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/members/${userId}`);
  }

  static async createInvitation(
    orgId: string,
    email: string,
    role: string
  ): Promise<OrgInvitation> {
    const response = await apiClient.post<OrgInvitation>(`/orgs/${orgId}/invitations`, {
      email,
      role
    });
    return response.data;
  }

  static async createBulkInvitations(
    orgId: string,
    invitations: Array<{ email: string; role: string }>
  ): Promise<OrgInvitation[]> {
    const response = await apiClient.post<{ invitations: OrgInvitation[] }>(
      `/orgs/${orgId}/invitations/bulk`,
      { invitations }
    );
    return response.data.invitations;
  }

  static async listInvitations(orgId: string): Promise<OrgInvitation[]> {
    const response = await apiClient.get<OrgInvitation[]>(`/orgs/${orgId}/invitations`);
    return response.data;
  }

  static async revokeInvitation(orgId: string, invitationId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/invitations/${invitationId}`);
  }

  static async acceptInvitation(token: string): Promise<Organization> {
    const response = await apiClient.post<Organization>(`/invitations/${token}/accept`);
    return response.data;
  }

  static async listMyInvitations(): Promise<MyInvitation[]> {
    const response = await apiClient.get<MyInvitation[]>("/invitations/mine");
    return response.data;
  }
}
