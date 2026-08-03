// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CustomAppSummary } from "@/types/apps";
import type { OrgRole } from "@/types/organization";
import LauncherPage from "./index";

/**
 * The page-level half of the access gate: the mapping from (org, role) to whether
 * the control renders at all.
 *
 * `AppCard.test.tsx` covers the card's half — that the button and badge follow the
 * `onManageAccess` prop. That leaves the line that actually decides whether an
 * operator ever sees it untested, which is the wrong half to leave uncovered: the
 * prop plumbing failing shows nothing to nobody, but this mapping failing shows the
 * control to a member.
 */

// Mutable so each test can pick who is looking. Read through the same selector
// shape zustand exposes, so the component's `useCurrentOrg((s) => s.role)` calls
// work unchanged.
let orgState: { org: { id: string } | null; role: OrgRole | null } = {
  org: { id: "org-1" },
  role: "member"
};

vi.mock("@/stores/useCurrentOrg", () => ({
  default: <T,>(selector: (s: typeof orgState) => T) => selector(orgState)
}));

vi.mock("@/stores/useAskDock", () => ({
  default: () => vi.fn()
}));

const app: CustomAppSummary = {
  id: "app-1",
  slug: "revenue",
  name: "Revenue",
  org_slug: "acme",
  url: "/customer-apps/acme/revenue/",
  published_at: "2026-07-30T00:00:00Z",
  visibility: "org"
};

vi.mock("@/hooks/api/customApps/useCustomApps", () => ({
  useCustomApps: () => ({ data: [app], isPending: false })
}));

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "ws-1" } })
}));

vi.mock("./useWorkspaceReadiness", () => ({
  default: () => ({ status: "ready", gaps: [], shouldDisableChat: false })
}));

// Chrome that isn't under test and drags in queries/stores of its own.
vi.mock("@/components/RecentThreads", () => ({ RecentThreads: () => null }));
vi.mock("./components/NeedsAttention", () => ({ NeedsAttention: () => null }));
vi.mock("./components/CriticalAlertBanner", () => ({ CriticalAlertBanner: () => null }));
vi.mock("./components/ProjectSetupToast", () => ({ ProjectSetupToast: () => null }));
vi.mock("./components/AskForwardFallback", () => ({ AskForwardFallback: () => null }));

// A marker, so "did the gate mount the dialog" is observable without standing up a
// QueryClientProvider for a dialog that is closed in every one of these cases.
vi.mock("@/components/appAccess/AppAccessDialog", () => ({
  AppAccessDialog: () => <div data-testid='access-dialog' />
}));

afterEach(cleanup);

const renderAs = (state: typeof orgState) => {
  orgState = state;
  render(
    <MemoryRouter>
      <LauncherPage />
    </MemoryRouter>
  );
};

const accessButton = () => screen.queryByRole("button", { name: /access/i });
const accessDialog = () => screen.queryByTestId("access-dialog");

describe("launcher access gate", () => {
  it.each<OrgRole>(["owner", "admin"])("offers the control to an org %s", (role) => {
    renderAs({ org: { id: "org-1" }, role });
    expect(accessButton()).toBeInTheDocument();
    expect(accessDialog()).toBeInTheDocument();
  });

  it("hides it from a plain member", () => {
    renderAs({ org: { id: "org-1" }, role: "member" });
    expect(accessButton()).toBeNull();
    // Not merely disabled or hidden — a member must not mount the dialog either.
    expect(accessDialog()).toBeNull();
  });

  it("hides it while the org store is still empty", () => {
    // The store is populated by OrgGuard, so the first render of a cold load has an
    // org of null with a role of null. Failing open here would flash the control on
    // every page load, for everyone.
    renderAs({ org: null, role: null });
    expect(accessButton()).toBeNull();
    expect(accessDialog()).toBeNull();
  });

  it("hides it when a role arrives without an org", () => {
    // Neither half is sufficient alone. `manageableOrgId` exists to make that one
    // decision rather than two that can disagree.
    renderAs({ org: null, role: "owner" });
    expect(accessButton()).toBeNull();
    expect(accessDialog()).toBeNull();
  });

  it("still renders the app card either way", () => {
    // The gate must subtract the control, not the page.
    renderAs({ org: { id: "org-1" }, role: "member" });
    expect(screen.getByTestId("launcher-app-card-revenue")).toBeInTheDocument();
  });
});
