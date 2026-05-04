import { apiClient } from "./axios";

export type BillingCycle = "monthly" | "annual";
export type BillingStatusId = "incomplete" | "active" | "past_due" | "unpaid" | "canceled";

export interface OrgBilling {
  status: BillingStatusId;
  billing_cycle: BillingCycle | null;
  seats_used: number;
  seats_paid: number;
  current_period_start: string | null;
  current_period_end: string | null;
  items: AdminSubscriptionItem[];
  grace_period_ends_at: string | null;
  payment_action_url: string | null;
}

/** Minimal payload returned by `GET /orgs/{id}/billing/status`. Readable by
 *  any org member; drives the FE paywall, banner, and OrgGuard. */
export interface OrgBillingStatus {
  status: BillingStatusId;
  grace_period_ends_at: string | null;
  payment_action_url: string | null;
}

export interface Invoice {
  id: string;
  amount_due: number;
  amount_paid: number;
  currency: string;
  status: string;
  hosted_invoice_url: string | null;
  period_start: number | null;
  period_end: number | null;
}

export class BillingService {
  static async get(orgId: string): Promise<OrgBilling> {
    const response = await apiClient.get<OrgBilling>(`/orgs/${orgId}/billing`);
    return response.data;
  }

  static async getStatus(orgId: string): Promise<OrgBillingStatus> {
    const response = await apiClient.get<OrgBillingStatus>(`/orgs/${orgId}/billing/status`);
    return response.data;
  }

  static async createPortalSession(orgId: string): Promise<{ url: string }> {
    const response = await apiClient.post<{ url: string }>(`/orgs/${orgId}/billing/portal-session`);
    return response.data;
  }

  static async listInvoices(orgId: string): Promise<Invoice[]> {
    const response = await apiClient.get<Invoice[]>(`/orgs/${orgId}/billing/invoices`);
    return response.data;
  }

  static async getCheckoutSession(orgId: string, sessionId: string): Promise<{ paid: boolean }> {
    const response = await apiClient.get<{ paid: boolean }>(
      `/orgs/${orgId}/billing/checkout-sessions/${sessionId}`
    );
    return response.data;
  }
}

// ---- Admin (sales-led provisioning) ----

export interface AdminOrgRow {
  id: string;
  slug: string;
  name: string;
  owner_email: string | null;
  status: BillingStatusId;
  created_at: string;
  stripe_subscription_id: string | null;
}

export interface AdminSubscriptionItem {
  id: string;
  quantity: number;
  price_id: string;
  price_nickname: string | null;
  unit_amount: number;
  currency: string;
  interval: string | null;
  product_name: string | null;
  current_period_start: number | null;
  current_period_end: number | null;
  amount_display: string;
}

export interface AdminSubscriptionDetail {
  id: string;
  status: string;
  livemode: boolean;
  created: number | null;
  current_period_start: number | null;
  current_period_end: number | null;
  cancel_at_period_end: boolean;
  collection_method: string | null;
  customer_id: string | null;
  items: AdminSubscriptionItem[];
  latest_invoice: LatestInvoiceSummary | null;
}

export interface LatestInvoiceSummary {
  id: string;
  status: string;
  collection_method: string | null;
  hosted_invoice_url: string | null;
  invoice_pdf: string | null;
  amount_due: number;
  amount_paid: number;
  currency: string;
  auto_advance: boolean | null;
  created: number | null;
  due_date: number | null;
  next_payment_attempt: number | null;
}

export interface AdminPriceDto {
  id: string;
  nickname: string | null;
  unit_amount: number;
  currency: string;
  interval: "month" | "year" | string;
  product_name: string | null;
  label: string;
  amount_display: string;
  billing_scheme: "per_unit" | "tiered" | string;
}

export type ProvisionItemRole = "seat" | "flat";

export interface ProvisionItem {
  price_id: string;
  role: ProvisionItemRole;
}

export interface ProvisionSubscriptionRequest {
  items: ProvisionItem[];
  days_until_due?: number;
}

export const DAYS_UNTIL_DUE_MIN = 7;
export const DAYS_UNTIL_DUE_MAX = 30;
export const DAYS_UNTIL_DUE_DEFAULT = DAYS_UNTIL_DUE_MAX;

export interface ProvisionSubscriptionResponse {
  provisioned: boolean;
  subscription_id: string;
  latest_invoice: LatestInvoiceSummary | null;
}

export interface ProvisionCheckoutRequest {
  items: ProvisionItem[];
}

export interface ProvisionCheckoutResponse {
  session_id: string;
  url: string;
  expires_at: number;
  email_sent_to: string | null;
  email_skipped: boolean;
  email_skip_reason: string | null;
}

export interface CheckoutPendingInfo {
  session_id: string;
  url: string;
  expires_at: number;
}

export interface CheckoutAlreadyPendingError {
  code: "checkout_already_pending";
  session_id: string;
  url: string;
  expires_at: number;
}

export class AdminBillingService {
  static async listOrgs(status?: BillingStatusId): Promise<AdminOrgRow[]> {
    const response = await apiClient.get<AdminOrgRow[]>("/admin/orgs", {
      params: status ? { status } : undefined
    });
    return response.data;
  }

  static async listPrices(): Promise<AdminPriceDto[]> {
    const response = await apiClient.get<AdminPriceDto[]>("/admin/billing/prices");
    return response.data;
  }

  static async getSubscription(orgId: string): Promise<AdminSubscriptionDetail> {
    const response = await apiClient.get<AdminSubscriptionDetail>(
      `/admin/orgs/${orgId}/billing/subscription`
    );
    return response.data;
  }

  static async provisionSubscription(
    orgId: string,
    body: ProvisionSubscriptionRequest
  ): Promise<ProvisionSubscriptionResponse> {
    const response = await apiClient.post<ProvisionSubscriptionResponse>(
      `/admin/orgs/${orgId}/billing/provision-subscription`,
      body
    );
    return response.data;
  }

  static async provisionCheckout(
    orgId: string,
    body: ProvisionCheckoutRequest
  ): Promise<ProvisionCheckoutResponse> {
    const response = await apiClient.post<ProvisionCheckoutResponse>(
      `/admin/orgs/${orgId}/billing/provision-checkout`,
      body
    );
    return response.data;
  }

  static async getPendingCheckout(orgId: string): Promise<CheckoutPendingInfo | null> {
    try {
      const response = await apiClient.get<CheckoutPendingInfo>(
        `/admin/orgs/${orgId}/billing/checkout`
      );
      return response.data;
    } catch (err) {
      if (isAxios404(err)) return null;
      throw err;
    }
  }

  static async resendCheckout(orgId: string): Promise<ProvisionCheckoutResponse> {
    const response = await apiClient.post<ProvisionCheckoutResponse>(
      `/admin/orgs/${orgId}/billing/checkout/resend`
    );
    return response.data;
  }

  static async cancelCheckout(orgId: string): Promise<void> {
    await apiClient.post(`/admin/orgs/${orgId}/billing/checkout/cancel`);
  }
}

function isAxios404(err: unknown): boolean {
  return (
    typeof err === "object" &&
    err !== null &&
    "response" in err &&
    (err as { response?: { status?: number } }).response?.status === 404
  );
}
