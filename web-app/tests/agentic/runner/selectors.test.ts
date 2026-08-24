import { describe, expect, it } from "vitest";
import {
  isNonDurableRecording,
  isSelectorTool,
  normalizeSelectorArgs,
  resolveAriaRefSelector
} from "./selectors";

// Regression coverage for the 2026-08-24 ide-save flake investigation
// (PR #2990): browser_snapshot's ariaSnapshot({mode:"ai"}) tags elements
// with refs the model naturally tries to click by, and the runtime must
// rewrite them to Playwright's real `aria-ref=` locator engine.

describe("resolveAriaRefSelector", () => {
  it.each([
    // Frame-prefixed (nonzero frameSeq — the shape seen in the real CI
    // failures once the page had navigated at least once).
    ["ref=f1e14", "aria-ref=f1e14"],
    ["[ref=f1e14]", "aria-ref=f1e14"],
    ["ref=f2e21", "aria-ref=f2e21"],
    ["f1e14", "aria-ref=f1e14"],
    ["[f1e14]", "aria-ref=f1e14"],
    // Unprefixed (frameSeq falsy — playwright-core's
    // ariaSnapshotWithRefs: refPrefix = frameSeq ? "f" + frameSeq : "").
    ["ref=e14", "aria-ref=e14"],
    ["[ref=e14]", "aria-ref=e14"],
    ["e14", "aria-ref=e14"],
    ["[e14]", "aria-ref=e14"],
    // Already the real engine — pass through unchanged.
    ["aria-ref=f1e14", "aria-ref=f1e14"],
    ["aria-ref=e14", "aria-ref=e14"]
  ])("rewrites %s to %s", (input, expected) => {
    expect(resolveAriaRefSelector(input)).toBe(expected);
  });

  it.each([
    "text=test.sql",
    "role=link[name='test.sql']",
    "[data-testid=ide-save-button]",
    "button:nth-of-type(1)",
    ".monaco-editor",
    "#credential-host"
  ])("leaves non-ref selector %s untouched", (input) => {
    expect(resolveAriaRefSelector(input)).toBe(input);
  });
});

describe("normalizeSelectorArgs", () => {
  it("rewrites a browser_click ref selector", () => {
    const out = normalizeSelectorArgs({ selector: "[ref=f1e14]" });
    expect(out.selector).toBe("aria-ref=f1e14");
  });

  it("rewrites a ref selector for a tool outside SELECTOR_TOOLS (e.g. browser_wait_for_selector)", () => {
    // browser_wait_for_selector is deliberately not in SELECTOR_TOOLS (it's
    // not state-changing, so it's never cached), but it still dispatches a
    // live page.locator() call and must not be exempted from the rewrite —
    // normalizeSelectorArgs no longer takes a tool name at all, precisely
    // so membership in SELECTOR_TOOLS can't gate it by accident.
    expect(isSelectorTool("browser_wait_for_selector")).toBe(false);
    const out = normalizeSelectorArgs({ selector: "ref=e14" });
    expect(out.selector).toBe("aria-ref=e14");
  });

  it("rewrites args.element as well as args.selector", () => {
    const out = normalizeSelectorArgs({ element: "[ref=f1e14]" });
    expect(out.element).toBe("aria-ref=f1e14");
  });

  it("returns the same object when there is nothing to normalize", () => {
    const args = { url: "/ide" };
    expect(normalizeSelectorArgs(args)).toBe(args);
  });

  it("does not mutate the input object", () => {
    const args = { selector: "[ref=f1e14]" };
    const out = normalizeSelectorArgs(args);
    expect(args.selector).toBe("[ref=f1e14]");
    expect(out).not.toBe(args);
  });
});

describe("isNonDurableRecording", () => {
  it("is true for an aria-ref primary with no durable strategy", () => {
    expect(
      isNonDurableRecording("aria-ref=e14", [{ kind: "css", selector: "aria-ref=e14", rank: 0 }])
    ).toBe(true);
  });

  it("is true for an aria-ref primary with no strategies at all", () => {
    expect(isNonDurableRecording("aria-ref=e14", undefined)).toBe(true);
  });

  it("is false when a durable strategy was also materialized", () => {
    expect(
      isNonDurableRecording("aria-ref=e14", [
        { kind: "testid", selector: "[data-testid=file-test-sql]", rank: 0 },
        { kind: "css", selector: "aria-ref=e14", rank: 1 }
      ])
    ).toBe(false);
  });

  it("is false for a non-ref primary regardless of strategies", () => {
    expect(isNonDurableRecording("text=test.sql", undefined)).toBe(false);
  });

  it("is false when there is no primary", () => {
    expect(isNonDurableRecording(undefined, undefined)).toBe(false);
  });
});
