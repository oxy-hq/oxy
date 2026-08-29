import { describe, expect, it } from "vitest";
import {
  canGoBack,
  canGoForward,
  currentEntry,
  EMPTY_HISTORY,
  isSameDocument,
  moveCursor,
  pushEntry,
  replaceEntry,
  shouldInterceptAnchor
} from "./previewHistory";

const build = (...urls: string[]) => urls.reduce(pushEntry, EMPTY_HISTORY);

describe("the preview's own stack", () => {
  it("starts with nothing to walk", () => {
    expect(canGoBack(EMPTY_HISTORY)).toBe(false);
    expect(canGoForward(EMPTY_HISTORY)).toBe(false);
    expect(currentEntry(EMPTY_HISTORY)).toBeNull();
  });

  it("walks back and forward like a browser", () => {
    const s = build("/a", "/b", "/c");
    expect(currentEntry(s)).toBe("/c");

    const back = moveCursor(s, -1);
    expect(currentEntry(back)).toBe("/b");
    expect(canGoForward(back)).toBe(true);

    expect(currentEntry(moveCursor(back, 1))).toBe("/c");
  });

  it("clamps rather than throwing at the ends", () => {
    // Two fast clicks on Back at the start of the stack is a normal gesture,
    // not an error.
    const s = build("/a");
    expect(currentEntry(moveCursor(s, -5))).toBe("/a");
    expect(currentEntry(moveCursor(s, 5))).toBe("/a");
  });

  it("discards the forward entries when you navigate after going back", () => {
    const s = pushEntry(moveCursor(build("/a", "/b", "/c"), -1), "/d");
    expect(s.entries).toEqual(["/a", "/b", "/d"]);
    expect(canGoForward(s)).toBe(false);
  });

  it("ignores a repeat of the location it is already on", () => {
    // An app that normalises its query string on every render would otherwise
    // fill the stack with entries the operator cannot tell apart, and Back
    // would look broken for the opposite reason.
    const s = pushEntry(build("/a"), "/a");
    expect(s.entries).toEqual(["/a"]);
  });

  it("replaceState rewrites in place and leaves the cursor alone", () => {
    const s = replaceEntry(build("/a", "/b"), "/b?x=1");
    expect(s.entries).toEqual(["/a", "/b?x=1"]);
    expect(s.index).toBe(1);
    expect(canGoBack(s)).toBe(true);
  });

  it("replaces into an empty stack rather than dropping the location", () => {
    expect(replaceEntry(EMPTY_HISTORY, "/a")).toEqual({ entries: ["/a"], index: 0 });
  });
});

describe("which clicks get converted to a replace", () => {
  const origin = "https://app.oxygen-hq.com";
  const anchor = (over: Partial<{ href: string; target: string; hasDownload: boolean }> = {}) => ({
    href: `${origin}/customer-apps/x/y/stores`,
    target: "",
    hasDownload: false,
    ...over
  });

  it("takes a plain same-origin link", () => {
    expect(shouldInterceptAnchor(anchor(), origin)).toBe(true);
    expect(shouldInterceptAnchor(anchor({ target: "_self" }), origin)).toBe(true);
  });

  it("leaves anything aimed at another browsing context", () => {
    // A target opens elsewhere, which is the one navigation that cannot
    // pollute this history — so intercepting it would only break it.
    expect(shouldInterceptAnchor(anchor({ target: "_blank" }), origin)).toBe(false);
    expect(shouldInterceptAnchor(anchor({ target: "named" }), origin)).toBe(false);
  });

  it("leaves a download alone", () => {
    expect(shouldInterceptAnchor(anchor({ hasDownload: true }), origin)).toBe(false);
  });

  it("leaves cross-origin and non-navigations alone", () => {
    expect(shouldInterceptAnchor(anchor({ href: "https://evil.example.com/" }), origin)).toBe(
      false
    );
    expect(shouldInterceptAnchor(anchor({ href: "mailto:a@b.c" }), origin)).toBe(false);
    expect(shouldInterceptAnchor(anchor({ href: "" }), origin)).toBe(false);
  });
});

describe("isSameDocument", () => {
  const at = (s: string) => `https://app.oxygen-hq.com/customer-apps/x/y/${s}`;

  it("is true when only the fragment differs", () => {
    // A fragment-only move must be absorbed with replaceState: assigning
    // `location.hash` adds a joint entry, and `location.replace` would reload
    // the document for a difference the document can handle itself.
    expect(isSameDocument(at("?a=1"), at("?a=1#top"))).toBe(true);
  });

  it("is false when the path or query differs", () => {
    expect(isSameDocument(at("?a=1"), at("?a=2"))).toBe(false);
    expect(isSameDocument(at("stores"), at("stores/20"))).toBe(false);
  });

  it("is false for junk rather than throwing", () => {
    expect(isSameDocument("not a url", at(""))).toBe(false);
  });
});
