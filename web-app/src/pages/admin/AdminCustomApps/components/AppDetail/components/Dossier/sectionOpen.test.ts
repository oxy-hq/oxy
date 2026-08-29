import { describe, expect, it } from "vitest";
import { isSectionOpen } from "./sectionOpen";

/**
 * The three-way interaction between the operator's stored collapse map, the
 * section a link named, and closing that section by hand. The middle one is an
 * override rather than a write, which is what made the last case a regression.
 */
describe("isSectionOpen", () => {
  it("honours the operator's own preference", () => {
    expect(isSectionOpen(true, "builds", null, null)).toBe(true);
    expect(isSectionOpen(false, "builds", null, null)).toBe(false);
  });

  it("opens the section a link named, without touching the others", () => {
    expect(isSectionOpen(false, "builds", "builds", null)).toBe(true);
    expect(isSectionOpen(false, "activity", "builds", null)).toBe(false);
  });

  /// THE regression. The override was unconditional, so closing the focused
  /// section wrote `false` into the stored map and the override re-opened it
  /// on the same render — a collapsible that does not respond, escapable only
  /// by editing the URL.
  it("lets the focused section be closed by hand", () => {
    expect(isSectionOpen(false, "builds", "builds", "builds")).toBe(false);
  });

  /// …and a second link opens its own section, because what is remembered is
  /// WHICH focus was dismissed rather than that one was.
  it("does not carry a dismissal onto the next link", () => {
    expect(isSectionOpen(false, "activity", "activity", "builds")).toBe(true);
  });

  it("still defers to a stored open after a dismissal", () => {
    // The operator expanded it themselves; the URL's dismissal is not a veto
    // over their own choice.
    expect(isSectionOpen(true, "builds", "builds", "builds")).toBe(true);
  });
});
