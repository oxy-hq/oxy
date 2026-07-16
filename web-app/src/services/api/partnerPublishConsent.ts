import { apiClient } from "./axios";

export interface PartnerPublishConsentStatus {
  enabled: boolean;
  /** Whether the caller may change it (a real org officer); false under the operator override. */
  can_manage: boolean;
}

/**
 * The client's opt-in for partner app publishing (`/orgs/{id}/partner-publish-consent`).
 * Default OFF. Only a real org Owner/Admin may set it; the server rejects the
 * synthetic-operator override on write.
 */
export const PartnerPublishConsentService = {
  async get(orgId: string): Promise<PartnerPublishConsentStatus> {
    const { data } = await apiClient.get(`/orgs/${orgId}/partner-publish-consent`);
    return data;
  },
  async set(orgId: string, enabled: boolean): Promise<PartnerPublishConsentStatus> {
    const { data } = await apiClient.put(`/orgs/${orgId}/partner-publish-consent`, { enabled });
    return data;
  }
};
