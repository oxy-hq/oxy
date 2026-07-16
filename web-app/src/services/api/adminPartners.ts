import type {
  AdminPartnerCapabilities,
  AdminPartnerDetail,
  AdminPartnerSummary,
  GrantPartnershipInput
} from "@/types/adminPartners";
import { apiClient } from "./axios";

/**
 * Staff partner provisioning (`/api/admin/partners`). Oxy grants a partnership to
 * an **existing org**, sets its **ceiling**, and attaches/detaches clients.
 *
 * Partner access (who is an operator) is normally the partner's own owner/admin's
 * job (see `PartnersService.setPersonAccess`), but staff can grant/revoke it here
 * too — to bootstrap or repair a partnership — and every such write is audited
 * `via_global_override`. Two-level governance: Oxy caps the partner; the partner
 * staffs itself.
 */
export const AdminPartnersService = {
  async list(): Promise<AdminPartnerSummary[]> {
    const { data } = await apiClient.get("/admin/partners");
    return data;
  },

  async get(orgId: string): Promise<AdminPartnerDetail> {
    const { data } = await apiClient.get(`/admin/partners/${orgId}`);
    return data;
  },

  /**
   * The ATOMIC grant: grant + ceiling + first client + first partner admin in ONE
   * server transaction, audited in-txn. A mid-flight failure leaves nothing behind.
   */
  async grant(input: GrantPartnershipInput): Promise<AdminPartnerDetail> {
    const { data } = await apiClient.post("/admin/partners/grant", input);
    return data;
  },

  /** Raise or lower the ceiling. `manage_billing` / `manage_secrets` are Owner-only. */
  async setCapabilities(
    orgId: string,
    capabilities: AdminPartnerCapabilities
  ): Promise<AdminPartnerCapabilities> {
    const { data } = await apiClient.put(`/admin/partners/${orgId}/capabilities`, capabilities);
    return data;
  },

  /** Attach an EXISTING org as a client. Staff-only — it hands over a live tenant. */
  async attachOrg(orgId: string, managedOrgId: string): Promise<AdminPartnerDetail> {
    const { data } = await apiClient.post(`/admin/partners/${orgId}/orgs`, {
      managed_org_id: managedOrgId
    });
    return data;
  },

  /** Staff-only, deliberately: a partner must not be able to orphan a customer. */
  async detachOrg(orgId: string, managedOrgId: string): Promise<void> {
    await apiClient.delete(`/admin/partners/${orgId}/orgs/${managedOrgId}`);
  },

  /** Withdraw the partnership. The org itself survives — it is a real tenant. */
  async revoke(orgId: string): Promise<void> {
    await apiClient.delete(`/admin/partners/${orgId}`);
  },

  /**
   * Grant or revoke partner access for a member of the partner org (staff
   * override). Returns the refreshed partner detail. Audited `via_global_override`.
   */
  async setPersonAccess(
    orgId: string,
    orgMemberId: string,
    hasAccess: boolean
  ): Promise<AdminPartnerDetail> {
    const url = `/admin/partners/${orgId}/people/${orgMemberId}`;
    const { data } = hasAccess ? await apiClient.put(url) : await apiClient.delete(url);
    return data;
  }
};
