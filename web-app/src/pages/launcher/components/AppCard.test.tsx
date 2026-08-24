// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import useAppDock from "@/stores/useAppDock";
import type { CustomAppSummary } from "@/types/apps";
import { AppCard } from "./AppCard";

afterEach(cleanup);

beforeEach(() => {
  useAppDock.setState({ app: null });
  document.head.innerHTML = "";
});

const app = (over: Partial<CustomAppSummary> = {}): CustomAppSummary => ({
  id: "app-1",
  slug: "revenue",
  name: "Revenue",
  org_slug: "acme",
  url: "/customer-apps/acme/revenue/",
  published_at: "2026-07-30T00:00:00Z",
  visibility: "org",
  ...over
});

describe("AppCard access control", () => {
  // The whole point of the prop: the launcher passes it only for org owners and
  // admins. A member's card must not mention access at all — not the button, and
  // not the badge that would tell them which apps are locked down.
  it("shows nothing about access without onManageAccess", () => {
    render(<AppCard app={app({ visibility: "members" })} />);
    expect(screen.queryByRole("button", { name: /access/i })).toBeNull();
    expect(screen.queryByText(/restricted/i)).toBeNull();
  });

  it("offers the control to a manager", async () => {
    const onManageAccess = vi.fn();
    render(<AppCard app={app()} onManageAccess={onManageAccess} />);
    await userEvent.click(screen.getByRole("button", { name: /access/i }));
    expect(onManageAccess).toHaveBeenCalledWith(expect.objectContaining({ id: "app-1" }));
  });

  it("badges only the restricted app, and only for a manager", () => {
    const { rerender } = render(<AppCard app={app()} onManageAccess={vi.fn()} />);
    // An open app is the default and true of most cards, so it gets no chip —
    // otherwise every tile carries a badge that says nothing.
    expect(screen.queryByText(/restricted/i)).toBeNull();

    rerender(<AppCard app={app({ visibility: "members" })} onManageAccess={vi.fn()} />);
    expect(screen.getByText(/restricted/i)).toBeInTheDocument();
  });

  it("keeps the card a link to the app", () => {
    render(<AppCard app={app()} onManageAccess={vi.fn()} />);
    // The name's link stretches over the card. Adding a second action must not
    // cost the card its one-click navigation.
    expect(screen.getByRole("link", { name: "Revenue" })).toHaveAttribute(
      "href",
      "/customer-apps/acme/revenue/"
    );
  });
});

describe("AppCard opening behaviour", () => {
  /// The default gesture keeps the user in HQ: the app opens in the right-hand
  /// dock rather than replacing the page they launched it from.
  it("docks the app on a plain click", async () => {
    render(<AppCard app={app()} />);
    await userEvent.click(screen.getByRole("link", { name: "Revenue" }));
    expect(useAppDock.getState().app?.id).toBe("app-1");
  });

  /// …and the modifier gestures still belong to the browser. Intercepting
  /// cmd-click is the single most common way an in-app router breaks "open in a
  /// new tab", so the handler defers rather than re-implementing it.
  it("leaves a cmd-click to the browser", async () => {
    // One `setup()` session, because modifier state lives on the session — the
    // bare `userEvent.click` helper starts a fresh one and would drop the Meta
    // key held by a preceding `userEvent.keyboard`, quietly turning this into a
    // second test of the plain-click path.
    const user = userEvent.setup();
    render(<AppCard app={app()} />);
    await user.keyboard("{Meta>}");
    await user.click(screen.getByRole("link", { name: "Revenue" }));
    await user.keyboard("{/Meta}");
    expect(useAppDock.getState().app).toBeNull();
  });

  it("offers an explicit new-tab escape hatch", () => {
    render(<AppCard app={app()} />);
    const external = screen.getByTestId("launcher-app-card-external-revenue");
    expect(external).toHaveAttribute("href", "/customer-apps/acme/revenue/");
    expect(external).toHaveAttribute("target", "_blank");
  });

  /// Hover warms the app's HTML — which also pulls its entry chunks via the
  /// response's preload hints — so the click lands on something already parsed.
  /// A `rel=prefetch` link rather than a `fetch()`, so the serve path can tell
  /// a hover from an open and not record a view for it.
  it("prefetches the app on hover and on keyboard focus", async () => {
    render(<AppCard app={app()} />);
    const prefetches = () =>
      document.head.querySelectorAll('link[rel="prefetch"][href="/customer-apps/acme/revenue/"]');

    await userEvent.hover(screen.getByRole("link", { name: "Revenue" }));
    expect(prefetches()).toHaveLength(1);

    // Reaching the card by Tab has to be as fast as reaching it by mouse — and
    // must not queue a second link for the same URL.
    screen.getByRole("link", { name: "Revenue" }).focus();
    expect(prefetches()).toHaveLength(1);
  });
});
