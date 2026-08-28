import { describe, expect, it } from "vitest";
import { appWindowName } from "./appWindowName";

describe("appWindowName", () => {
  /// The whole point of a name rather than `_blank`: the same app resolves to
  /// the same tab, so a second click re-targets it instead of stacking a copy.
  it("is stable for one app", () => {
    expect(appWindowName("poke-house", "bookkeeping")).toBe(
      appWindowName("poke-house", "bookkeeping")
    );
  });

  /// An app slug is unique **within an org**, but every org shares one origin
  /// and therefore one window-name space. Keyed on the slug alone, two tenants
  /// that both call an app `bookkeeping` would share a tab — so a user in more
  /// than one org (staff in an assume-role session, a partner across downstream
  /// orgs, a consultant in two tenants) could have the tab they were working in
  /// navigated from one tenant's app to another's, possibly in the background.
  it("does not collide across orgs that reuse an app slug", () => {
    expect(appWindowName("poke-house", "bookkeeping")).not.toBe(
      appWindowName("acme", "bookkeeping")
    );
  });
});
