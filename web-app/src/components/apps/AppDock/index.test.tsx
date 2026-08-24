// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useNavigate } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import useAppDock from "@/stores/useAppDock";
import type { CustomAppSummary } from "@/types/apps";
import { AppDock } from "./index";

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

const openWith = (a: CustomAppSummary = app(), focus = true) =>
  useAppDock.setState({ app: a, focus });

/// The dock reads the current route (it closes when you navigate away), so every
/// render needs a router around it.
const dock = (path = "/") => (
  <MemoryRouter initialEntries={[path]}>
    <AppDock />
  </MemoryRouter>
);
const renderDock = (path = "/") => render(dock(path));

describe("AppDock", () => {
  beforeEach(() => {
    localStorage.clear();
    useAppDock.setState({ app: null, focus: true });
  });

  /// Unmounted, not collapsed to width 0 like the Ask dock — a hidden iframe
  /// keeps running the app's timers, polling, and SSE streams for a pane nobody
  /// is looking at.
  it("renders nothing while closed", () => {
    const { container } = renderDock();
    expect(container).toBeEmptyDOMElement();
  });

  it("frames the app it was opened with", () => {
    openWith();
    renderDock();
    const frame = screen.getByTitle("Revenue");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame).toHaveAttribute("src", "/customer-apps/acme/revenue/");
    expect(screen.getByTestId("app-dock")).toBeInTheDocument();
  });

  it("closes from the header", async () => {
    openWith();
    renderDock();
    await userEvent.click(screen.getByTestId("app-dock-close"));
    expect(useAppDock.getState().app).toBeNull();
  });

  /// Escape is the reflex for "get me out of this". It must not fire while the
  /// frame itself has focus, though — a dialog the app opened should close
  /// before its host does.
  it("closes on Escape from the shell, not from inside the app", async () => {
    openWith();
    renderDock();

    screen.getByTitle("Revenue").focus();
    await userEvent.keyboard("{Escape}");
    expect(useAppDock.getState().app).not.toBeNull();

    screen.getByTestId("app-dock-close").focus();
    await userEvent.keyboard("{Escape}");
    expect(useAppDock.getState().app).toBeNull();
  });

  /// A real anchor, so middle-click, cmd-click and "copy link address" work —
  /// this is the escape hatch to a full tab and it has to behave like a link.
  it("offers a real new-tab link to the app", () => {
    openWith();
    renderDock();
    const popout = screen.getByTestId("app-dock-popout");
    expect(popout).toHaveAttribute("href", "/customer-apps/acme/revenue/");
    expect(popout).toHaveAttribute("target", "_blank");
    expect(popout).toHaveAttribute("rel", "noreferrer");
  });

  it("toggles focus mode from the header", async () => {
    openWith(app(), true);
    renderDock();
    expect(screen.getByTestId("app-dock")).toHaveAttribute("data-focus", "on");
    // In focus mode the split is not the user's to resize — the dock is the
    // whole content column and `<main>` is hidden.
    expect(screen.queryByTestId("app-dock-resize")).toBeNull();

    await userEvent.click(screen.getByTestId("app-dock-focus"));
    expect(useAppDock.getState().focus).toBe(false);
    expect(screen.getByTestId("app-dock")).toHaveAttribute("data-focus", "off");
    expect(screen.getByTestId("app-dock-resize")).toBeInTheDocument();
  });

  /// Switching apps must replace the frame rather than navigate the old one —
  /// otherwise the previous app's history stack and unload handlers come along.
  it("replaces the frame when a different app is docked", () => {
    openWith();
    const { rerender } = renderDock();
    const first = screen.getByTitle("Revenue");

    openWith(app({ id: "app-2", slug: "costs", name: "Costs", url: "/customer-apps/acme/costs/" }));
    rerender(dock());

    const second = screen.getByTitle("Costs");
    expect(second).not.toBe(first);
    expect(second).toHaveAttribute("src", "/customer-apps/acme/costs/");
    expect(screen.queryByTitle("Revenue")).toBeNull();
  });

  it("reloads by remounting the frame, keeping the same app", async () => {
    openWith();
    renderDock();
    const before = screen.getByTitle("Revenue");

    await userEvent.click(screen.getByTestId("app-dock-reload"));

    const after = screen.getByTitle("Revenue");
    expect(after).not.toBe(before);
    expect(after).toHaveAttribute("src", "/customer-apps/acme/revenue/");
  });
});

describe("AppDock route scoping", () => {
  beforeEach(() => {
    localStorage.clear();
    useAppDock.setState({ app: null, focus: true });
  });

  /// A real navigation, not a re-render with different `initialEntries` —
  /// `MemoryRouter` builds its history once at mount, so re-rendering it with a
  /// new entry list changes nothing and the test would pass on a dock that
  /// never reads the route at all.
  function Navigator({ to }: { to: string }) {
    const navigate = useNavigate();
    return (
      <button type='button' data-testid='go' onClick={() => navigate(to)}>
        go
      </button>
    );
  }

  const renderWithNav = (to: string) =>
    render(
      <MemoryRouter initialEntries={["/"]}>
        <AppDock />
        <Navigator to={to} />
      </MemoryRouter>
    );

  /// Focus mode hides `<main>`, so a dock that outlived its page would leave
  /// someone who clicked another rail item looking at the same app with the
  /// page they asked for rendered invisibly behind it. It is also an iframe
  /// still running an app nobody is watching.
  it("closes when the route changes underneath it", async () => {
    openWith();
    renderWithNav("/threads");
    expect(screen.getByTestId("app-dock")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("go"));
    expect(useAppDock.getState().app).toBeNull();
  });

  it("stays open while the route holds still", async () => {
    openWith();
    // Navigating to the path already showing is a no-op the router collapses,
    // so this asserts the dock does not close on any incidental re-render.
    renderWithNav("/");
    await userEvent.click(screen.getByTestId("go"));
    expect(useAppDock.getState().app?.id).toBe("app-1");
  });
});
