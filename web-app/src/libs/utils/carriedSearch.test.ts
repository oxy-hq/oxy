import { describe, expect, it } from "vitest";
import { carriedSearch } from "./carriedSearch";

const carry = (query: string) =>
  carriedSearch(new URLSearchParams(query), ["settings", "notifications"]);

describe("carriedSearch", () => {
  it("keeps only the named params", () => {
    expect(carry("settings=account.tokens&slack_installed=ok")).toBe("?settings=account.tokens");
    expect(carry("notifications=open")).toBe("?notifications=open");
    expect(carry("settings=account.tokens&notifications=open")).toBe(
      "?settings=account.tokens&notifications=open"
    );
  });

  it("is empty when there is nothing to keep", () => {
    expect(carry("")).toBe("");
    expect(carry("other=1")).toBe("");
    // An empty value names nothing.
    expect(carry("settings=")).toBe("");
  });
});
