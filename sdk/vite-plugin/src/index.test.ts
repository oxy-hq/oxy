import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { resolveBase, validateManifest } from "./index.js";

describe("validateManifest", () => {
  test("accepts a v2 identity-only manifest", () => {
    expect(
      validateManifest({ schemaVersion: 2, slug: "my-app", name: "My App" })
    ).toEqual([]);
  });

  test("accepts a manifest with optional orgSlug and projectId hints", () => {
    expect(
      validateManifest({
        schemaVersion: 2,
        slug: "my-app",
        orgSlug: "acme",
        projectId: "00000000-0000-0000-0000-000000000001"
      })
    ).toEqual([]);
  });

  test("rejects v1 schemaVersion", () => {
    const errs = validateManifest({ schemaVersion: 1, slug: "x" });
    expect(errs.some((e) => e.includes("schemaVersion"))).toBe(true);
  });

  test("rejects v1 leakage (products field)", () => {
    const errs = validateManifest({
      schemaVersion: 2,
      slug: "x",
      products: { foo: {} }
    });
    expect(errs.some((e) => e.includes("v1 fields"))).toBe(true);
  });

  test("rejects v1 leakage (writers field)", () => {
    const errs = validateManifest({
      schemaVersion: 2,
      slug: "x",
      writers: { foo: {} }
    });
    expect(errs.some((e) => e.includes("v1 fields"))).toBe(true);
  });

  test("rejects missing slug", () => {
    const errs = validateManifest({ schemaVersion: 2 });
    expect(errs.some((e) => e.includes("slug is required"))).toBe(true);
  });

  test("rejects malformed slug (uppercase)", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "MyApp" });
    expect(errs.some((e) => e.includes("malformed"))).toBe(true);
  });

  test("rejects malformed slug (starts with dash)", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "-bad" });
    expect(errs.some((e) => e.includes("malformed"))).toBe(true);
  });

  // The cases that used to build cleanly and then 422 at `oxy publish` — the
  // whole reason to catch them at build.
  test("rejects malformed slug (trailing dash)", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "app-" });
    expect(errs.some((e) => e.includes("malformed"))).toBe(true);
  });

  test("rejects malformed slug (double dash)", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "a--b" });
    expect(errs.some((e) => e.includes("malformed"))).toBe(true);
  });

  test("rejects malformed slug (underscore)", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "my_app" });
    expect(errs.some((e) => e.includes("malformed"))).toBe(true);
  });

  test("rejects malformed slug (over 63 chars)", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "a".repeat(64) });
    expect(errs.some((e) => e.includes("malformed"))).toBe(true);
  });

  test("accepts a valid hyphenated slug", () => {
    const errs = validateManifest({ schemaVersion: 2, slug: "oltp-bookings" });
    expect(errs.some((e) => e.includes("malformed"))).toBe(false);
  });

  test("rejects a path-traversal function name", () => {
    const errs = validateManifest({
      schemaVersion: 2,
      slug: "app",
      functions: { "../../x": { route: true } }
    });
    expect(errs.some((e) => e.includes("function name") && e.includes("malformed"))).toBe(true);
  });

  test("rejects functions declared as an array", () => {
    const errs = validateManifest({
      schemaVersion: 2,
      slug: "app",
      functions: []
    });
    expect(errs.some((e) => e.includes("functions must be an object"))).toBe(true);
  });

  test("accepts a valid functions map", () => {
    const errs = validateManifest({
      schemaVersion: 2,
      slug: "app",
      functions: { notify: { route: true } }
    });
    expect(errs.some((e) => e.includes("function name"))).toBe(false);
  });

  test("rejects missing schemaVersion as if it were v1", () => {
    const errs = validateManifest({ slug: "x" });
    expect(errs.some((e) => e.includes("schemaVersion"))).toBe(true);
  });
});

describe("resolveBase", () => {
  const savedEnv = process.env.OXY_APP_BASE_PATH;
  beforeEach(() => {
    delete process.env.OXY_APP_BASE_PATH;
  });
  afterEach(() => {
    if (savedEnv === undefined) delete process.env.OXY_APP_BASE_PATH;
    else process.env.OXY_APP_BASE_PATH = savedEnv;
  });

  test("env var wins over manifest", () => {
    process.env.OXY_APP_BASE_PATH = "/customer-apps/from-env/x/";
    expect(
      resolveBase({ schemaVersion: 2, slug: "from-manifest", orgSlug: "y" })
    ).toBe("/customer-apps/from-env/x/");
  });

  test("env var gets a trailing slash if absent", () => {
    process.env.OXY_APP_BASE_PATH = "/customer-apps/x/y";
    expect(resolveBase(null)).toBe("/customer-apps/x/y/");
  });

  test("derives base from manifest orgSlug + slug when env unset", () => {
    expect(
      resolveBase({ schemaVersion: 2, slug: "store-pulse", orgSlug: "acme" })
    ).toBe("/customer-apps/acme/store-pulse/");
  });

  test("falls back to / when neither env nor manifest hints exist", () => {
    expect(resolveBase({ schemaVersion: 2, slug: "x" })).toBe("/");
    expect(resolveBase(null)).toBe("/");
  });

  test("manifest with only one of orgSlug/slug also falls back to /", () => {
    expect(resolveBase({ schemaVersion: 2, slug: "x" })).toBe("/");
    expect(
      resolveBase({ schemaVersion: 2, slug: "x", orgSlug: "" })
    ).toBe("/");
  });
});
