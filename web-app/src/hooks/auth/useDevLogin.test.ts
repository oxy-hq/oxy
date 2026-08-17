// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { sanitizeNextPath } from "./useDevLogin";

// `/dev-login?next=…` exists so a browser-automation run lands on the page
// under test in one navigation. It must stay a same-origin jump: the page is
// public and unauthenticated by design, so an unsanitized `next` would make it
// an open redirect that also hands over a fresh session.
describe("sanitizeNextPath", () => {
  it("accepts a same-origin path", () => {
    expect(sanitizeNextPath("/ide")).toBe("/ide");
    expect(sanitizeNextPath("/threads/abc?tab=runs")).toBe("/threads/abc?tab=runs");
  });

  it("rejects an absolute URL", () => {
    expect(sanitizeNextPath("https://evil.example.com/")).toBeNull();
  });

  it("rejects a protocol-relative URL", () => {
    expect(sanitizeNextPath("//evil.example.com/")).toBeNull();
  });

  it("rejects a missing or empty value", () => {
    expect(sanitizeNextPath(null)).toBeNull();
    expect(sanitizeNextPath(undefined)).toBeNull();
    expect(sanitizeNextPath("")).toBeNull();
  });
});
