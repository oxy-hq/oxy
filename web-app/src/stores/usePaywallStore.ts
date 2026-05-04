import { create } from "zustand";
import type { BillingStatusId } from "@/services/api/billing";

// Holds the most recent `subscription_required` 402 surfaced by the axios
// interceptor so `PaywallScreen` can mount with the right copy variant.
// `contactRequired` reflects the backend's `contact_required` flag — when
// true (the post-2026-04-28 sales-gated default) the paywall shows a
// contact-sales CTA with no Subscribe button. When false (a future
// self-serve mode) it would show pricing + Subscribe.
type PaywallStatus = Extract<BillingStatusId, "incomplete" | "unpaid" | "canceled">;

interface PaywallState {
  open: boolean;
  status: PaywallStatus;
  contactRequired: boolean;
  show: (status: PaywallStatus, contactRequired: boolean) => void;
  close: () => void;
}

export const usePaywallStore = create<PaywallState>((set) => ({
  open: false,
  status: "incomplete",
  contactRequired: true,
  show: (status, contactRequired) => set({ open: true, status, contactRequired }),
  close: () => set({ open: false })
}));
