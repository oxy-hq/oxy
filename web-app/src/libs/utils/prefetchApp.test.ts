// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import { prefetchApp } from "./prefetchApp";

const links = () => Array.from(document.head.querySelectorAll('link[rel="prefetch"]'));

describe("prefetchApp", () => {
  beforeEach(() => {
    document.head.innerHTML = "";
  });

  it("adds a document prefetch link for the app", () => {
    prefetchApp("/customer-apps/acme/revenue/");
    expect(links()).toHaveLength(1);
    const link = links()[0] as HTMLLinkElement;
    expect(link.getAttribute("href")).toBe("/customer-apps/acme/revenue/");
    // `as=document` is what makes the browser treat this as a navigation target
    // and honour the response's own preload hints, rather than parking bytes.
    expect(link.getAttribute("as")).toBe("document");
  });

  /// A card fires this on every pointer enter. Without de-duplication `<head>`
  /// accumulates one link per hover for the life of the page.
  it("is idempotent per URL", () => {
    prefetchApp("/customer-apps/acme/revenue/");
    prefetchApp("/customer-apps/acme/revenue/");
    prefetchApp("/customer-apps/acme/costs/");
    expect(links()).toHaveLength(2);
  });

  it("ignores an empty URL", () => {
    prefetchApp("");
    expect(links()).toHaveLength(0);
  });
});
