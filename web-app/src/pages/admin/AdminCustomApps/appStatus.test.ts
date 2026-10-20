import { describe, expect, it } from "vitest";
import type { AppHealth, CustomApp } from "@/types/apps";
import {
  ATTENTION,
  byAttention,
  type HealthIndex,
  lifecycleVerdict,
  matchesQuery,
  needsAttention,
  STATUS_TONE,
  statusOf,
  statusRank
} from "./appStatus";

describe("lifecycleVerdict", () => {
  // Every row of the table in the doc comment, so the comment and the code cannot
  // drift. A reader checking a verdict on screen should find its row here.
  it("calls an unpublished app a draft whatever was measured", () => {
    const v = lifecycleVerdict({
      published: false,
      health: "operational",
      availability: { verdict: "healthy" }
    });
    expect(v).toEqual({ status: "draft", tone: "muted", label: "Draft", source: "draft" });
  });

  it.each(["down", "degraded", "not_measured", "quiet", "operational"] as const)(
    "reports the fleet's %s verdict in its own tone when availability does not escalate",
    (health) => {
      for (const availability of [
        null,
        { verdict: "healthy" as const },
        { verdict: "no_opinion" as const }
      ]) {
        const v = lifecycleVerdict({ published: true, health, availability });
        expect(v.status).toBe(health);
        expect(v.tone).toBe(STATUS_TONE[health]);
        expect(v.source).toBe("fleet");
      }
    }
  );

  it("lets a burn escalate not_measured, quiet and operational — worse wins", () => {
    for (const health of ["not_measured", "quiet", "operational"] as const) {
      expect(
        lifecycleVerdict({
          published: true,
          health,
          availability: { verdict: "burning", severity: "page" }
        })
      ).toEqual({ status: "down", tone: "danger", label: "Burning", source: "availability" });
      expect(
        lifecycleVerdict({
          published: true,
          health,
          availability: { verdict: "burning", severity: "ticket" }
        })
      ).toEqual({ status: "degraded", tone: "warn", label: "Degraded", source: "availability" });
    }
  });

  it("never softens down or degraded, whatever the last hour's ratio says", () => {
    for (const health of ["down", "degraded"] as const) {
      const v = lifecycleVerdict({
        published: true,
        health,
        availability: { verdict: "healthy" }
      });
      expect(v.status).toBe(health);
      expect(v.source).toBe("fleet");
    }
  });

  it("answers from availability alone when the fleet has not", () => {
    expect(lifecycleVerdict({ published: true, availability: { verdict: "healthy" } })).toEqual({
      status: "operational",
      tone: "ok",
      label: "Serving",
      source: "availability"
    });
    // `no_opinion` survives: it is "no data", which is not a flavour of healthy.
    expect(lifecycleVerdict({ published: true, availability: { verdict: "no_opinion" } })).toEqual({
      status: "quiet",
      tone: "muted",
      label: "No data",
      source: "availability"
    });
    expect(
      lifecycleVerdict({
        published: true,
        availability: { verdict: "burning", severity: "ticket" }
      })
    ).toMatchObject({ status: "degraded", tone: "warn" });
  });

  it("states only the publish fact when nothing measured the app", () => {
    const v = lifecycleVerdict({ published: true });
    expect(v).toEqual({ status: null, tone: "muted", label: "Published", source: "none" });
    // `null` keeps the unknown where `statusOf` already ranks it.
    expect(statusRank(v.status)).toBe(statusRank(null));
  });

  describe("with the server's verdict", () => {
    it.each(["down", "degraded", "not_measured", "quiet", "operational"] as const)(
      "reads %s from the verdict when no fleet row has arrived",
      (verdict) => {
        const v = lifecycleVerdict({ published: true, verdict });
        expect(v.status).toBe(verdict);
        expect(v.tone).toBe(STATUS_TONE[verdict]);
        expect(v.source).toBe("fleet");
      }
    );

    it("prefers the polled fleet row over a list verdict that disagrees — it is fresher", () => {
      const v = lifecycleVerdict({ published: true, verdict: "operational", health: "down" });
      expect(v.status).toBe("down");
    });

    it("still lets a burn escalate it — worse wins", () => {
      expect(
        lifecycleVerdict({
          published: true,
          verdict: "operational",
          availability: { verdict: "burning", severity: "page" }
        })
      ).toEqual({ status: "down", tone: "danger", label: "Burning", source: "availability" });
    });

    it("never turns no_opinion into a healthy verdict", () => {
      // The availability read has no opinion and the server sent a quiet verdict:
      // the surface says so, muted, rather than borrowing a green tick.
      expect(
        lifecycleVerdict({
          published: true,
          verdict: "quiet",
          availability: { verdict: "no_opinion" }
        })
      ).toEqual({ status: "quiet", tone: "muted", label: "Quiet", source: "fleet" });
    });

    it("does not let a stale draft verdict un-publish an app that says it is published", () => {
      // `draft` carries no health, so the client mapping decides: here, nothing measured.
      expect(lifecycleVerdict({ published: true, verdict: "draft" })).toEqual({
        status: null,
        tone: "muted",
        label: "Published",
        source: "none"
      });
    });

    it("keeps a draft a draft whatever the verdict says", () => {
      expect(lifecycleVerdict({ published: false, verdict: "operational" }).status).toBe("draft");
    });
  });
});

/**
 * The judgement salvaged from the deleted registry table. Two things matter more than
 * the ordering itself:
 *
 * - **unknown is not a verdict.** `statusOf` returns `null` when health has not
 *   arrived, and nothing may turn that into `not_measured` — that would claim capture
 *   is off for an app nobody asked about.
 * - **`byAttention` orders the fleet**, so its first row is what an operator reads as
 *   "the thing to look at". A tie that reorders between reloads would reshuffle the
 *   list under them. It was briefly load-bearing *navigation* too — the bare route
 *   redirected to `[0]` — and the ordering outlived that redirect unchanged.
 */

const app = (over: Partial<CustomApp> = {}): CustomApp =>
  ({
    id: over.id ?? `id-${Math.random()}`,
    slug: "oxy-starter",
    name: "Oxy Starter",
    org_slug: "acme",
    published_at: "2026-09-19T17:30:27Z",
    ...over
  }) as CustomApp;

const health = (...pairs: Array<[string, AppHealth]>): HealthIndex =>
  new Map(pairs.map(([id, h]) => [id, { health: h }]));

describe("statusOf", () => {
  it("calls an unpublished app a draft before it looks at health at all", () => {
    const a = app({ id: "a", published_at: null });
    expect(statusOf(a, health(["a", "operational"]))).toBe("draft");
  });

  it("reports the health verdict for a published app", () => {
    const a = app({ id: "a" });
    expect(statusOf(a, health(["a", "degraded"]))).toBe("degraded");
  });

  it("prefers the polled health row over the list row's verdict — a left-open table stays current", () => {
    expect(statusOf(app({ id: "a", verdict: "operational" }), health(["a", "down"]))).toBe("down");
  });

  it("uses the row's server verdict when the health index has not answered", () => {
    expect(statusOf(app({ id: "a", verdict: "down" }), undefined)).toBe("down");
    expect(statusOf(app({ id: "a", verdict: "down" }), health(["other", "operational"]))).toBe(
      "down"
    );
    expect(statusOf(app({ id: "a", published_at: null, verdict: "draft" }), undefined)).toBe(
      "draft"
    );
    // A verdict is an answer, so a row that has one is known even with no health index.
    expect(statusOf(app({ id: "a", verdict: "not_measured" }), undefined)).toBe("not_measured");
  });

  it("falls back to the client mapping when the row carries no verdict", () => {
    // An older server, or a cheap single response: exactly the behaviour before the field.
    expect(statusOf(app({ id: "a" }), health(["a", "degraded"]))).toBe("degraded");
    expect(statusOf(app({ id: "a", published_at: null }), undefined)).toBe("draft");
  });

  it("returns null — not a verdict — when health has not arrived", () => {
    expect(statusOf(app({ id: "a" }), undefined)).toBeNull();
    expect(statusOf(app({ id: "a" }), health(["other", "operational"]))).toBeNull();
  });
});

describe("needsAttention", () => {
  it("wants a person for exactly the three verdicts the backend names", () => {
    for (const s of ["down", "degraded", "not_measured"] as const) {
      expect(ATTENTION.has(s)).toBe(true);
      expect(needsAttention(s)).toBe(true);
    }
  });

  it("does not want a person for a healthy, quiet or draft app", () => {
    for (const s of ["operational", "quiet", "draft"] as const)
      expect(needsAttention(s)).toBe(false);
  });

  it("does not want a person for an app nobody measured", () => {
    // `null` means "we do not know", which is not the same as "not measured" — that
    // distinction is the whole reason `statusOf` can return null.
    //
    // Honest note: this assertion does NOT pin the `s !== null` guard in the
    // implementation. `Set.prototype.has(null)` is already false, so deleting the guard
    // leaves runtime behaviour identical and this test still green — verified by
    // mutating it. The guard is there for the type checker (`ATTENTION.has` takes
    // `AppStatus`, not `AppStatus | null`), and the type checker is what pins it.
    expect(needsAttention(null)).toBe(false);
  });
});

describe("statusRank", () => {
  it("ranks worst first, with unknown between the verdicts and drafts", () => {
    const order = (
      ["down", "degraded", "not_measured", "quiet", "operational", null, "draft"] as const
    ).map(statusRank);
    expect(order).toEqual([...order].sort((a, b) => a - b));
    expect(statusRank(null)).toBeGreaterThan(statusRank("operational"));
    expect(statusRank(null)).toBeLessThan(statusRank("draft"));
  });
});

describe("byAttention", () => {
  it("puts the app that most needs someone first — this is what /admin/apps opens on", () => {
    const down = app({ id: "d", name: "Zeta" });
    const fine = app({ id: "f", name: "Alpha" });
    const sorted = byAttention([fine, down], health(["d", "down"], ["f", "operational"]));
    expect(sorted[0].id).toBe("d");
  });

  it("breaks ties on name then org, so a reload lands in the same place", () => {
    const h = health(["1", "down"], ["2", "down"], ["3", "down"]);
    const apps = [
      app({ id: "1", name: "Beta", org_slug: "acme" }),
      app({ id: "3", name: "Alpha", org_slug: "zzz" }),
      app({ id: "2", name: "Alpha", org_slug: "acme" })
    ];
    const once = byAttention(apps, h).map((a) => a.id);
    const twice = byAttention([...apps].reverse(), h).map((a) => a.id);
    expect(once).toEqual(["2", "3", "1"]);
    expect(twice).toEqual(once);
  });

  it("does not mutate the fleet it was handed", () => {
    const apps = [app({ id: "a", name: "Z" }), app({ id: "b", name: "A" })];
    byAttention(apps, undefined);
    expect(apps.map((a) => a.id)).toEqual(["a", "b"]);
  });

  it("still orders a fleet whose health never arrived", () => {
    const apps = [app({ id: "a", name: "Z" }), app({ id: "b", name: "A" })];
    expect(byAttention(apps, undefined).map((a) => a.name)).toEqual(["A", "Z"]);
  });
});

describe("matchesQuery", () => {
  const a = app({ name: "Oxy Starter", slug: "oxy-starter", org_slug: "acme" });

  it("matches on name, slug and org, because name alone does not identify an app", () => {
    for (const q of ["oxy", "starter", "acme", "OXY", "  acme  "])
      expect(matchesQuery(a, q)).toBe(true);
  });

  it("matches the compound org/slug an operator would paste from a URL", () => {
    expect(matchesQuery(a, "acme/oxy-starter")).toBe(true);
  });

  it("narrows between two apps that share a name", () => {
    const local = app({ name: "Oxy Starter", slug: "oxy-starter", org_slug: "local" });
    expect(matchesQuery(a, "acme")).toBe(true);
    expect(matchesQuery(local, "acme")).toBe(false);
  });

  it("matches everything on an empty query", () => {
    expect(matchesQuery(a, "")).toBe(true);
    expect(matchesQuery(a, "   ")).toBe(true);
  });
});
