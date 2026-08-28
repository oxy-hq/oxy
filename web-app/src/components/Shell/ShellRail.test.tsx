// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { type RailItem, ShellRail } from "./ShellRail";

afterEach(cleanup);

const appItem: RailItem = {
  key: "app-1",
  label: "Bookkeeping",
  testId: "rail-app-bookkeeping",
  letter: "B",
  href: "/customer-apps/poke-house/bookkeeping/",
  newTab: "oxy-app-poke-house-bookkeeping"
};

describe("ShellRail app entries", () => {
  /// A custom app is a separate document with its own router and its own
  /// query-string navigation state. Opened in this tab it interleaves its
  /// history with HQ's (Back stops landing where you came from) and takes over
  /// the address bar, so its own deep links are unreachable and a reload lands
  /// on HQ. Its own tab is what gives the app back its URL.
  it("opens an app in its own named window", () => {
    render(<ShellRail groups={[[appItem]]} />);
    const link = screen.getByTestId("rail-app-bookkeeping");
    expect(link).toHaveAttribute("href", "/customer-apps/poke-house/bookkeeping/");
    // Named, not `_blank`: clicking the tile again re-targets the tab already
    // open rather than stacking a second copy of the same app.
    expect(link).toHaveAttribute("target", "oxy-app-poke-house-bookkeeping");
    // A bare icon is the whole affordance, so the only place a screen-reader
    // user can learn this leaves the tab is the accessible name.
    expect(link).toHaveAccessibleName("Bookkeeping (opens in a new tab)");
  });

  /// The rail also carries in-SPA destinations as anchors in other surfaces;
  /// those must stay in this tab, so the target is opt-in per item.
  it("keeps an href entry without newTab in this tab", () => {
    render(<ShellRail groups={[[{ ...appItem, newTab: undefined }]]} />);
    const link = screen.getByTestId("rail-app-bookkeeping");
    expect(link).not.toHaveAttribute("target");
    // …and says nothing about a new tab, because it does not open one.
    expect(link).toHaveAccessibleName("Bookkeeping");
  });
});
