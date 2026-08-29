import { describe, expect, it } from "vitest";
import { applying, type DeepLinkHandoff, landed, report } from "./deepLinkHandoff";

describe("the deep-link handoff", () => {
  it("publishes everything while idle", () => {
    expect(report(null, "/stores")).toEqual({ publish: true, next: null });
    expect(report(null, null)).toEqual({ publish: true, next: null });
  });

  /// The transient this exists for: opening `?preview=/stores` loads the
  /// bundle root first, and publishing that blanks the param until the real
  /// document lands.
  it("suppresses the document a deep link is passing through", () => {
    const h = applying("/stores");
    expect(report(h, "/")).toEqual({ publish: false, next: "/stores" });
  });

  it("publishes and clears when the link lands where it was aimed", () => {
    expect(report(applying("/stores"), "/stores")).toEqual({ publish: true, next: null });
  });

  /// THE regression. An exact match was the only exit, so a deep-linked app
  /// that rewrites its own URL on mount (`/stores` → `/stores?tab=all`) never
  /// matched — and every later report was dropped for the life of the
  /// component, across navigations, because the ref outlived them. A new
  /// document landing has to end the handoff whatever it landed on.
  it("ends the handoff on a new document, even one it did not aim for", () => {
    let h: DeepLinkHandoff = applying("/stores");
    expect(report(h, "/")).toEqual({ publish: false, next: "/stores" });

    h = landed(); // the applied navigation produced a document
    expect(report(h, "/stores?tab=all")).toEqual({ publish: true, next: null });
    // …and it stays open afterwards, which is the property that was lost.
    expect(report(null, "/reports")).toEqual({ publish: true, next: null });
  });

  it("keeps two deep links apart", () => {
    // Held as a value, not a flag: "in flight to /stores" must not absorb the
    // report that /reports has landed.
    expect(report(applying("/stores"), "/reports")).toEqual({
      publish: false,
      next: "/stores"
    });
  });
});
