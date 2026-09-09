import { describe, expect, it, vi } from "vitest";
import { signOut, signOutUrl } from "./shellContext";

describe("signOutUrl", () => {
  it("uses the product's login page and returns to the app", () => {
    expect(
      signOutUrl(
        "https://acme.oxygen-hq.com/login",
        "https://acme--ops.customer-apps.oxygen-hq.com/"
      )
    ).toBe(
      "https://acme.oxygen-hq.com/login?return_to=https%3A%2F%2Facme--ops.customer-apps.oxygen-hq.com%2F"
    );
  });
  it("falls back to the same-origin login without a shell context", () => {
    expect(signOutUrl(undefined, "https://x.test/app/")).toBe(
      "/login?return_to=https%3A%2F%2Fx.test%2Fapp%2F"
    );
    expect(signOutUrl("  ", "https://x.test/app/")).toBe(
      "/login?return_to=https%3A%2F%2Fx.test%2Fapp%2F"
    );
  });
  it("appends to a login URL that already carries a query", () => {
    expect(signOutUrl("https://h.test/login?org=acme", "https://h.test/a/")).toBe(
      "https://h.test/login?org=acme&return_to=https%3A%2F%2Fh.test%2Fa%2F"
    );
  });
});

describe("signOut", () => {
  it("throws on a refused logout and does not navigate", async () => {
    const fetcher = vi.fn(async () => new Response(null, { status: 401 }));
    await expect(signOut(fetcher, "https://h.test/login", "https://h.test/a/")).rejects.toThrow(
      "HTTP 401"
    );
    expect(fetcher).toHaveBeenCalledWith(
      "/api/logout",
      expect.objectContaining({ method: "GET", credentials: "include" })
    );
  });
});
