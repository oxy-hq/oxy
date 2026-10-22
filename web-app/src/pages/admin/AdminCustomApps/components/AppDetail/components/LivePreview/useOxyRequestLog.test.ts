import { describe, expect, it } from "vitest";
import { invocationIdOf, isCapturedUrl, isFunctionCallUrl } from "./useOxyRequestLog";

describe("isFunctionCallUrl", () => {
  it("matches a call to one of the app's functions and nothing else of the app", () => {
    expect(isFunctionCallUrl("/customer-apps/a/b/fn/x")).toBe(true);
    expect(isFunctionCallUrl("http://127.0.0.1:5173/customer-apps/acme/pulse-demo/fn/store")).toBe(
      true
    );
    expect(isFunctionCallUrl("/customer-apps/a/b/fn/x?refresh=1")).toBe(true);
    expect(isFunctionCallUrl("/customer-apps/a/b/assets/x.js")).toBe(false);
    expect(isFunctionCallUrl("/customer-apps/a/b/")).toBe(false);
    expect(isFunctionCallUrl("/customer-apps/a/b/fn/")).toBe(false);
    // A function name in a query string is not a function call.
    expect(isFunctionCallUrl("/customer-apps/a/b/page?next=/customer-apps/a/b/fn/x")).toBe(false);
  });

  it("keeps capturing the API, and adds function calls to it", () => {
    expect(isCapturedUrl("/api/customer-apps/x/query")).toBe(true);
    expect(isCapturedUrl("/customer-apps/a/b/fn/x")).toBe(true);
    expect(isCapturedUrl("/customer-apps/a/b/assets/app.js")).toBe(false);
  });
});

describe("invocationIdOf", () => {
  it("reads the invocation the answer names, whatever the header's case", () => {
    expect(invocationIdOf({ resHeaders: { "x-oxy-invocation-id": "iv-1" } })).toBe("iv-1");
    expect(invocationIdOf({ resHeaders: { "X-Oxy-Invocation-Id": "iv-2" } })).toBe("iv-2");
    expect(invocationIdOf({ resHeaders: { "content-type": "text/event-stream" } })).toBeUndefined();
    expect(invocationIdOf({})).toBeUndefined();
  });
});
