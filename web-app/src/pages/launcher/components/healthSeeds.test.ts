import { describe, expect, it } from "vitest";
import type { AppVerdict, CustomAppSummary } from "@/types/apps";
import { healthSeeds } from "./healthSeeds";

const app = (slug: string, verdict?: AppVerdict): CustomAppSummary => ({
  id: `id-${slug}`,
  slug,
  name: `App ${slug}`,
  org_slug: "acme",
  url: `https://x.test/customer-apps/acme/${slug}/`,
  published_at: "2026-10-01T00:00:00Z",
  visibility: "org",
  verdict
});

describe("healthSeeds", () => {
  it("raises down as critical and degraded as a warning, addressed to the app", () => {
    const seeds = healthSeeds([app("a", "down"), app("b", "degraded")]);
    expect(seeds.map((s) => [s.id, s.severity, s.target])).toEqual([
      ["health-a", "critical", { app: "a" }],
      ["health-b", "warning", { app: "b" }]
    ]);
    expect(seeds[0].title).toContain("App a");
  });

  it("surfaces an unmeasured app, quietly — no reading is never healthy", () => {
    const [seed] = healthSeeds([app("a", "not_measured")]);
    expect(seed.severity).toBe("info");
    expect(seed.category).toBe("Not measured");
  });

  it.each(["quiet", "operational", "draft", undefined] as const)(
    "raises nothing for %s — silence is not a healthy verdict",
    (verdict) => {
      expect(healthSeeds([app("a", verdict)])).toEqual([]);
    }
  );

  it("ignores the free-text launcher status line entirely", () => {
    const withStatus = { ...app("a", "down"), status: "23 stores · live" };
    expect(healthSeeds([withStatus])[0].severity).toBe("critical");
    expect(healthSeeds([{ ...app("a"), status: "all green · live" }])).toEqual([]);
  });
});
