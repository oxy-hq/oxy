import { describe, expect, it } from "vitest";
import { dossierAppId, dossierAppMatchesRoute } from "./dossierWindowApp";

describe("dossierAppId", () => {
  it("accepts a UUID, normalised", () => {
    expect(dossierAppId(" 0F3B2C1A-9D8E-4F7A-8B6C-5D4E3F2A1B0C ")).toBe(
      "0f3b2c1a-9d8e-4f7a-8b6c-5d4e3f2a1b0c"
    );
  });

  it.each([
    ["path traversal", "../../admin"],
    ["a slash", "abc/def"],
    ["a slug", "oxy-starter"],
    ["an encoded traversal", "..%2F..%2Fadmin"],
    ["empty", ""],
    ["absent", null]
  ])("drops %s so it never reaches a request URL", (_, raw) => {
    expect(dossierAppId(raw)).toBeUndefined();
  });
});

describe("dossierAppMatchesRoute", () => {
  const app = { org_slug: "acme", slug: "store" };

  it("accepts the app the address names", () => {
    expect(dossierAppMatchesRoute(app, "acme", "store")).toBe(true);
  });

  it.each([
    ["another slug", "acme", "other"],
    ["another org", "globex", "store"],
    ["no route params", undefined, undefined]
  ])("refuses %s", (_, org, slug) => {
    expect(dossierAppMatchesRoute(app, org, slug)).toBe(false);
  });
});
