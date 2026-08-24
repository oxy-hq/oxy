// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import type { CustomAppSummary } from "@/types/apps";
import useAppDock from "./useAppDock";
import useAskDock from "./useAskDock";

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

describe("useAppDock", () => {
  beforeEach(() => {
    localStorage.clear();
    useAppDock.setState({ app: null, focus: true });
    useAskDock.setState({ isOpen: false });
  });

  it("opens and closes around one app", () => {
    useAppDock.getState().open(app());
    expect(useAppDock.getState().app?.id).toBe("app-1");

    useAppDock.getState().open(app({ id: "app-2", name: "Costs" }));
    expect(useAppDock.getState().app?.id).toBe("app-2");

    useAppDock.getState().close();
    expect(useAppDock.getState().app).toBeNull();
  });

  /// Both docks are right-hand siblings of `<main>`; two open at once would
  /// leave the page with no page. Ask loses nothing by being closed — its own
  /// `close()` is the state-preserving one.
  it("closes the Ask dock when an app opens", () => {
    useAskDock.getState().open({ message: "half-typed question" });
    expect(useAskDock.getState().isOpen).toBe(true);

    useAppDock.getState().open(app());
    expect(useAskDock.getState().isOpen).toBe(false);
    // …and the composer prefill survives, because Ask was closed rather than reset.
    expect(useAskDock.getState().prefill?.message).toBe("half-typed question");
  });

  it("remembers the focus choice across stores", () => {
    expect(useAppDock.getState().focus).toBe(true);
    useAppDock.getState().toggleFocus();
    expect(useAppDock.getState().focus).toBe(false);
    expect(localStorage.getItem("oxy:app-dock-focus")).toBe("0");

    useAppDock.getState().toggleFocus();
    expect(localStorage.getItem("oxy:app-dock-focus")).toBe("1");
  });

  /// Closing the dock must not discard the user's focus preference — they will
  /// open another app in a moment and expect the same layout.
  it("keeps focus across an open/close cycle", () => {
    useAppDock.getState().toggleFocus();
    useAppDock.getState().open(app());
    useAppDock.getState().close();
    expect(useAppDock.getState().focus).toBe(false);
  });
});
