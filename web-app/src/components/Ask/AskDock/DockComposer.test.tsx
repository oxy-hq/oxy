// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CustomAppSummary } from "@/types/apps";
import { DockComposer } from "./DockComposer";

/**
 * The chips above the composer must come from the workspace being looked at,
 * and from nowhere else.
 *
 * This is a tenancy test wearing the costume of a UI test. The chips were a
 * hardcoded array written against one customer's evals, so every other tenant
 * was shown that customer's business questions — the failure mode is not "wrong
 * copy", it is one client reading another client's domain in their own
 * workspace. The case that matters most is therefore the EMPTY one: a workspace
 * that has declared nothing must show nothing, because any fallback we could
 * write there is some tenant's data.
 */

let apps: CustomAppSummary[] = [];
let pending = false;
const open = vi.fn();

vi.mock("@/hooks/api/customApps/useCustomApps", () => ({
  useCustomApps: () => ({ data: pending ? undefined : apps, isPending: pending })
}));

vi.mock("@/stores/useAskDock", () => {
  const store = <T,>(selector: (s: { prefill: null; open: typeof open }) => T) =>
    selector({ prefill: null, open });
  store.getState = () => ({ openThread: vi.fn() });
  return { default: store };
});

vi.mock("@/stores/useCurrentOrg", () => ({
  default: <T,>(selector: (s: { org: { name: string } }) => T) =>
    selector({ org: { name: "Acme" } })
}));

vi.mock("@/stores/useCurrentWorkspace", () => ({
  default: <T,>(selector: (s: { workspace: { id: string } }) => T) =>
    selector({ workspace: { id: "ws-1" } })
}));

// The composer itself is not under test — it pulls in the editor stack, which
// jsdom does not need to render for this assertion.
vi.mock("@/components/Chat/ChatPanel", () => ({ default: () => <div data-testid='chat-panel' /> }));

const app = (suggested_questions?: string[], default_agent?: string): CustomAppSummary => ({
  id: "app-1",
  slug: "inventory",
  name: "Inventory",
  org_slug: "acme",
  url: "/customer-apps/acme/inventory/",
  published_at: "2026-08-30T00:00:00Z",
  visibility: "org",
  suggested_questions,
  default_agent
});

/** The chips row, or null. Asserting on the WRAPPER rather than on a button
 *  count: `ChatPanel` is mocked out, so counting buttons would also pass if the
 *  row itself started rendering empty. */
const chipsRow = () => screen.queryByTestId("ask-suggestions");

afterEach(() => {
  cleanup();
  open.mockClear();
  apps = [];
  pending = false;
});

describe("DockComposer suggestions", () => {
  it("shows the questions this workspace's apps declare", () => {
    apps = [app(["Which SKUs are out of stock?", "What should we send this week?"])];
    render(<DockComposer />);
    expect(screen.getByText("Which SKUs are out of stock?")).toBeTruthy();
    expect(screen.getByText("What should we send this week?")).toBeTruthy();
  });

  it("shows nothing when the workspace has declared none", () => {
    // No fallback, deliberately. Whatever we could put here would be some other
    // tenant's domain — which is the bug this test exists to prevent, not a gap
    // in it.
    apps = [app(undefined)];
    render(<DockComposer />);
    expect(chipsRow()).toBeNull();
  });

  it("shows nothing when the workspace has no apps at all", () => {
    apps = [];
    render(<DockComposer />);
    expect(chipsRow()).toBeNull();
  });

  it("renders no row while the apps are still loading", () => {
    // An empty row that fills in later shoves the composer down.
    pending = true;
    render(<DockComposer />);
    expect(chipsRow()).toBeNull();
  });

  it("caps at three and de-duplicates across apps, ignoring whitespace", () => {
    // Several apps in one workspace repeat their headline question, sometimes
    // with incidental padding. Trimmed dedup keeps that from eating two slots.
    apps = [app(["a", "b"]), app([" b ", "c", "d"])];
    render(<DockComposer />);
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual(["a", "b", "c"]);
  });

  it("sends each question to the agent of the app that declared it", () => {
    // `ask.agent` and `ask.suggestedQuestions` are authored together; a question
    // answered by the wrong agent is a confident chip with a confused answer.
    apps = [app(["Why does Pleasanton rank #1?"], "agents/site_scout.agentic.yml")];
    render(<DockComposer />);
    screen.getByText("Why does Pleasanton rank #1?").click();
    expect(open).toHaveBeenCalledWith({
      message: "Why does Pleasanton rank #1?",
      agentPath: "agents/site_scout.agentic.yml"
    });
  });
});
