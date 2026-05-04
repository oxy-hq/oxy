// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PaywallScreen } from "./PaywallScreen";

// Mock the data hooks so the paywall renders deterministically with no
// network calls. Pricing must NEVER appear in any variant — this test is
// the regression guard for the central "no public pricing" guarantee of the
// 2026-04-28 sales-gated redesign.
vi.mock("@/hooks/api/billing", () => ({
  useOrgBillingStatus: () => ({
    data: {
      status: "incomplete",
      grace_period_ends_at: null,
      payment_action_url: null
    }
  }),
  useCreatePortalSession: () => ({ mutate: vi.fn(), isPending: false })
}));

vi.mock("@/stores/usePaywallStore", () => ({
  usePaywallStore: (sel: (s: { status: string }) => unknown) => sel({ status: "incomplete" })
}));

vi.mock("@/stores/useCurrentOrg", () => ({
  default: (sel: (s: { org: { id: string } | undefined }) => unknown) =>
    sel({ org: { id: "org-1" } })
}));

afterEach(() => cleanup());

const wrap = (ui: React.ReactNode) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{ui}</QueryClientProvider>;
};

describe("PaywallScreen — no-pricing regression (2026-04-28 sales-gated)", () => {
  it.each([
    ["admin", true],
    ["member", false]
  ] as const)("renders no pricing for %s", (_label, isAdmin) => {
    const { container } = render(wrap(<PaywallScreen isAdmin={isAdmin} />));
    const text = container.textContent ?? "";
    // Hard regression checks. If a future refactor re-introduces a public
    // pricing surface inside PaywallScreen, one of these will trip.
    expect(text).not.toMatch(/\$\d/);
    expect(text.toLowerCase()).not.toContain("subscribe");
    expect(text).not.toMatch(/\/seat/i);
    expect(text).not.toMatch(/\/month/i);
    expect(text).not.toMatch(/\/year/i);
  });
});
