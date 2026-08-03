// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CustomAppSummary } from "@/types/apps";
import { AppCard } from "./AppCard";

afterEach(cleanup);

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
