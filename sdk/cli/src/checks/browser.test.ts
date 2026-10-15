/**
 * The browser check's pure parts: which requests get the credential and the
 * environment header, which responses are `/fn` calls, and the verdict.
 * Playwright itself is not loaded here.
 */

import { describe, expect, it } from "vitest";
import {
  type BrowserTarget,
  deniedCallOf,
  functionCallOf,
  headersFor,
  type Observed,
  scoreBrowserCheck
} from "./browser.js";
import { browserTarget } from "./browser-step.js";

const APP = { appId: "a1", label: "acme/store", orgSlug: "acme", appSlug: "store" };

function target(overrides: Partial<BrowserTarget> = {}): BrowserTarget {
  return {
    ...browserTarget(
      { target: "https://oxy.test/", bearer: "tok" },
      APP,
      { appEnv: "dev-a1", timeoutSeconds: 300 },
      "build-a1"
    ),
    ...overrides
  };
}

function observed(overrides: Partial<Observed> = {}): Observed {
  return {
    ready: true,
    pageErrors: [],
    app: { environment: "dev-a1", buildId: "build-a1" },
    functionCalls: [],
    ...overrides
  };
}

describe("browserTarget", () => {
  it("loads the path URL and sends the credential and the environment to its origin", () => {
    const t = target();
    expect(t.url).toBe("https://oxy.test/customer-apps/acme/store/");
    expect(t.origin).toBe("https://oxy.test");
    expect(t.headers).toEqual({ authorization: "Bearer tok", "x-oxy-app-env": "dev-a1" });
    expect(t.waitMs).toBe(30_000);
  });

  it("sends an API key when no bearer resolves, and no header for production", () => {
    const t = browserTarget(
      { target: "https://oxy.test", headers: { "X-API-Key": "k" } },
      APP,
      { timeoutSeconds: 10 },
      undefined
    );
    expect(t.headers).toEqual({ "x-api-key": "k" });
    expect(t.environment).toBe("production");
    expect(t.waitMs).toBe(10_000);
  });
});

describe("headersFor", () => {
  it("adds the headers under the app's base only — never a CDN or another host", () => {
    const t = target();
    expect(t.basePath).toBe("/customer-apps/acme/store/");
    expect(headersFor("https://oxy.test/customer-apps/acme/store/", t)).toEqual(t.headers);
    expect(headersFor("https://oxy.test/customer-apps/acme/store/fn/echo", t)).toEqual(t.headers);
    expect(headersFor("https://oxy.test/customer-apps/acme/store/__oxy/beacon", t)).toEqual(
      t.headers
    );
    expect(headersFor("https://fonts.example.com/a.woff2", t)).toBeUndefined();
    expect(headersFor("https://acme--store.oxy.test/", t)).toBeUndefined();
    expect(headersFor("not a url", t)).toBeUndefined();
  });

  // The app's code runs on the deployment's origin, beside the whole API: a
  // credential on its other requests would let the build under test act as
  // the person running the check.
  it("never adds them to the rest of the API on the same origin", () => {
    const t = target();
    for (const url of [
      "https://oxy.test/api/admin/apps",
      "https://oxy.test/api/admin/apps/a1/promote",
      "https://oxy.test/api/auth/tokens",
      "https://oxy.test/api/projects/p/query",
      "https://oxy.test/customer-apps/acme/other/fn/echo",
      "https://oxy.test/customer-apps/acme/store-two/",
      "https://oxy.test/customer-apps/acme/store/../../../api/admin/apps",
      "https://oxy.test/customer-apps/acme/store/%2e%2e/%2e%2e/%2e%2e/api/admin/apps"
    ]) {
      expect(headersFor(url, t), url).toBeUndefined();
    }
  });
});

describe("functionCallOf", () => {
  it("names a /fn call on the app's origin with the invocation it wrote", () => {
    expect(
      functionCallOf(
        "https://oxy.test/customer-apps/acme/store/fn/echo",
        200,
        { "x-oxy-invocation-id": "inv-1" },
        target()
      )
    ).toEqual({ function: "echo", status: 200, invocationId: "inv-1" });
  });

  it("ignores assets and another origin's /fn", () => {
    expect(
      functionCallOf("https://oxy.test/customer-apps/acme/store/assets/fn.js", 200, {}, target())
    ).toBeUndefined();
    expect(
      functionCallOf("https://other.test/customer-apps/acme/store/fn/echo", 200, {}, target())
    ).toBeUndefined();
  });
});

describe("scoreBrowserCheck", () => {
  it("passes a page that mounted in the named environment on its build", () => {
    const result = scoreBrowserCheck(target(), observed(), 1200);
    expect(result).toMatchObject({
      name: "browser",
      kind: "browser",
      status: "done",
      passed: true
    });
    expect(result.error).toBeUndefined();
  });

  it("fails a page another environment served, however well it mounted", () => {
    const result = scoreBrowserCheck(
      target(),
      observed({ app: { environment: "production", buildId: "live" } }),
      1
    );
    expect(result.passed).toBe(false);
    expect(result.error).toMatch(/served by production, not dev-a1/);
  });

  it("fails a page served from another build than the environment's", () => {
    const result = scoreBrowserCheck(
      target(),
      observed({ app: { environment: "dev-a1", buildId: "old" } }),
      1
    );
    expect(result.passed).toBe(false);
    expect(result.error).toMatch(/build old, but dev-a1 serves build-a1/);
  });

  it("times out without the ready beacon, and fails on a page error or a missing selector", () => {
    expect(scoreBrowserCheck(target(), observed({ ready: false }), 1).status).toBe("timed_out");
    expect(scoreBrowserCheck(target(), observed({ pageErrors: ["boom"] }), 1).error).toMatch(
      /boom/
    );
    expect(
      scoreBrowserCheck(target(), observed({ selectorError: "#x was not visible" }), 1).error
    ).toMatch(/#x/);
    expect(
      scoreBrowserCheck(target(), observed({ navigationError: "the page answered 404" }), 1).error
    ).toBe("the page answered 404");
  });

  // The check withholds the credential from the page's own /api calls (F1);
  // a failure that may follow says so instead of blaming the app.
  it("names the refused calls the withheld credential caused when the page fails", () => {
    const denied = ["/api/projects/p/query answered 401"];
    const selector = scoreBrowserCheck(
      target(),
      observed({ selectorError: ".revenue was not visible within 30s", denied }),
      1
    );
    expect(selector.error).toMatch(/\.revenue was not visible/);
    expect(selector.error).toMatch(/\/api\/projects\/p\/query answered 401/);
    expect(selector.error).toMatch(/no credential outside the app's base path/);
    expect(scoreBrowserCheck(target(), observed({ ready: false, denied }), 1).error).toMatch(
      /answered 401/
    );
    // A retried or polled call is named once.
    const polled = scoreBrowserCheck(
      target(),
      observed({
        ready: false,
        denied: [...Array(5).fill(denied[0]), "/api/projects/p/semantic-query answered 401"]
      }),
      1
    );
    expect(polled.error?.match(/query answered 401/g)).toHaveLength(2);
    expect(polled.error).not.toMatch(/more\)/);
    // A page that passes anyway passes, with nothing added.
    const passed = scoreBrowserCheck(target(), observed({ denied }), 1);
    expect(passed.passed).toBe(true);
    expect(passed.error).toBeUndefined();
  });
});

describe("deniedCallOf", () => {
  it("records a refused same-origin call outside the app's base, and nothing else", () => {
    const t = target();
    expect(deniedCallOf("https://oxy.test/api/projects/p/query", 401, t)).toBe(
      "/api/projects/p/query answered 401"
    );
    expect(deniedCallOf("https://oxy.test/api/admin/apps", 403, t)).toBe(
      "/api/admin/apps answered 403"
    );
    expect(deniedCallOf("https://oxy.test/api/projects/p/query", 200, t)).toBeUndefined();
    expect(deniedCallOf("https://oxy.test/api/projects/p/query", 500, t)).toBeUndefined();
    expect(deniedCallOf("https://oxy.test/customer-apps/acme/store/fn/x", 401, t)).toBeUndefined();
    expect(deniedCallOf("https://cdn.example.com/x", 403, t)).toBeUndefined();
  });
});
