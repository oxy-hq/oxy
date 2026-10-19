import { describe, expect, it } from "vitest";
import type { AppBuild, AppBuildHistory } from "@/types/apps";
import { deriveLifecycle, type LifecycleAppRef } from "./deriveLifecycle";

/**
 * The derivation behind the lifecycle card. The card is the one place an operator
 * reads "what is live, is it OK, what can I promote or roll back" — so each of those
 * three has to be pinned here against the shapes the backend actually returns, not
 * eyeballed in a browser.
 */

const app = (over: Partial<LifecycleAppRef> = {}): LifecycleAppRef => ({
  id: "app-1",
  org_slug: "acme",
  slug: "store",
  published_at: "2026-09-19T17:30:27Z",
  last_promoted_at: "2026-09-19T17:30:27Z",
  last_promoted_by_email: "row@oxy.tech",
  ...over
});

const build = (over: Partial<AppBuild>): AppBuild => ({
  id: over.id ?? "b",
  build_id: over.build_id ?? "0123456789abcdef",
  created_at: "2026-09-19T17:00:00Z",
  is_draft: false,
  is_published: false,
  ...over
});

const history = (builds: AppBuild[], over: Partial<AppBuildHistory> = {}): AppBuildHistory => ({
  builds,
  ...over
});

const base = {
  health: null,
  availability: null,
  held: null,
  heldUnavailable: false,
  isLoading: false,
  error: null
};

describe("deriveLifecycle — what is live", () => {
  it("names the live and draft builds from the channel flags", () => {
    const live = build({ id: "live", is_published: true });
    const draft = build({ id: "draft", is_draft: true });
    const l = deriveLifecycle({ ...base, app: app(), history: history([draft, live]) });
    expect(l.liveBuild?.id).toBe("live");
    expect(l.draftBuild?.id).toBe("draft");
    expect(l.draftAhead).toBe(true);
  });

  it("is not draft-ahead when the draft is the live build", () => {
    const same = build({ id: "same", is_published: true, is_draft: true });
    const l = deriveLifecycle({ ...base, app: app(), history: history([same]) });
    expect(l.draftAhead).toBe(false);
  });

  it("is draft-ahead when something is drafted and nothing is live yet", () => {
    const draft = build({ id: "draft", is_draft: true });
    const l = deriveLifecycle({
      ...base,
      app: app({ published_at: null }),
      history: history([draft])
    });
    expect(l.liveBuild).toBeNull();
    expect(l.draftAhead).toBe(true);
  });

  it("has nothing to promote with no builds at all", () => {
    const l = deriveLifecycle({ ...base, app: app(), history: history([]) });
    expect(l.draftAhead).toBe(false);
    expect(l.builds).toEqual([]);
  });

  it("prefers the history's promoter over the registry row's, and falls back", () => {
    const fromHistory = deriveLifecycle({
      ...base,
      app: app(),
      history: history([], { promoted_at: "2026-10-01T00:00:00Z", promoted_by_email: "h@x" })
    });
    expect(fromHistory.lastPromoted).toEqual({ at: "2026-10-01T00:00:00Z", byEmail: "h@x" });

    const fromRow = deriveLifecycle({ ...base, app: app(), history: undefined });
    expect(fromRow.lastPromoted).toEqual({ at: "2026-09-19T17:30:27Z", byEmail: "row@oxy.tech" });
  });
});

describe("deriveLifecycle — is it OK", () => {
  it("reads draft off published_at before any measurement", () => {
    const l = deriveLifecycle({
      ...base,
      app: app({ published_at: null }),
      history: undefined,
      health: "operational"
    });
    expect(l.published).toBe(false);
    expect(l.verdict.status).toBe("draft");
  });

  it("escalates a fleet verdict on a page-severity burn, and never softens one", () => {
    const burning = deriveLifecycle({
      ...base,
      app: app(),
      history: undefined,
      health: "operational",
      availability: { verdict: "burning", severity: "page" }
    });
    expect(burning.verdict).toMatchObject({ status: "down", tone: "danger" });

    const down = deriveLifecycle({
      ...base,
      app: app(),
      history: undefined,
      health: "down",
      availability: { verdict: "healthy" }
    });
    expect(down.verdict).toMatchObject({ status: "down", source: "fleet" });
  });

  it("keeps no_opinion as no data, never a green tick", () => {
    const l = deriveLifecycle({
      ...base,
      app: app(),
      history: undefined,
      availability: { verdict: "no_opinion" }
    });
    expect(l.verdict).toMatchObject({ status: "quiet", tone: "muted", label: "No data" });
  });
});

describe("deriveLifecycle — held writes", () => {
  it("counts writes across entries, not entries", () => {
    const l = deriveLifecycle({
      ...base,
      app: app(),
      history: undefined,
      held: [{ writes: [1, 2] }, { writes: [3] }]
    });
    expect(l.heldCount).toBe(3);
    expect(l.heldTruncated).toBe(false);
  });

  it("has no count — not zero — when the list is unavailable or not yet read", () => {
    expect(
      deriveLifecycle({ ...base, app: app(), history: undefined, held: null }).heldCount
    ).toBeNull();
    expect(
      deriveLifecycle({
        ...base,
        app: app(),
        history: undefined,
        held: [],
        heldUnavailable: true
      }).heldCount
    ).toBeNull();
  });

  it("reads a full list as a floor", () => {
    const full = Array.from({ length: 100 }, () => ({ writes: [1] }));
    const l = deriveLifecycle({ ...base, app: app(), history: undefined, held: full });
    expect(l.heldTruncated).toBe(true);
    expect(l.heldCount).toBe(100);
  });
});
