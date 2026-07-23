import { describe, expect, it } from "vitest";
import type { CustomApp } from "@/types/apps";
import {
  type AppsTableState,
  buildAppsTableModel,
  DEFAULT_TABLE_STATE,
  fleetStats
} from "./useAppsTable";

/** Minimal CustomApp with sensible defaults; override per test. */
function app(over: Partial<CustomApp>): CustomApp {
  return {
    id: over.id ?? over.slug ?? "id",
    slug: "app",
    name: "App",
    org_id: "org-id",
    org_slug: "acme",
    project_id: "proj-0000",
    branch: "main",
    source_repo: "",
    status: "active",
    url: "/customer-apps/acme/app/",
    url_subdomain: null,
    source_type: "s3",
    source_config: {},
    bootstrap_pr_url: null,
    last_synced_at: null,
    last_deploy_at: null,
    published_at: null,
    repo_path: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over
  };
}

const state = (over: Partial<AppsTableState> = {}): AppsTableState => ({
  ...DEFAULT_TABLE_STATE,
  ...over
});

describe("buildAppsTableModel", () => {
  it("filters by search across name / slug / org / project", () => {
    const apps = [
      app({ id: "a", name: "Sales", slug: "sales", org_slug: "acme" }),
      app({ id: "b", name: "Ops", slug: "ops", org_slug: "globex" })
    ];
    expect(buildAppsTableModel(apps, state({ q: "globex", group: "none" })).flatIds).toEqual(["b"]);
    expect(buildAppsTableModel(apps, state({ q: "sales", group: "none" })).flatIds).toEqual(["a"]);
  });

  it("filters by status (live = has published_at)", () => {
    const apps = [
      app({ id: "live", published_at: "2026-02-01T00:00:00Z" }),
      app({ id: "draft", published_at: null })
    ];
    expect(buildAppsTableModel(apps, state({ status: "live", group: "none" })).flatIds).toEqual([
      "live"
    ]);
    expect(buildAppsTableModel(apps, state({ status: "draft", group: "none" })).flatIds).toEqual([
      "draft"
    ]);
  });

  it("filters by source type", () => {
    const apps = [app({ id: "v", source_type: "v0" }), app({ id: "s", source_type: "s3" })];
    expect(buildAppsTableModel(apps, state({ source: "v0", group: "none" })).flatIds).toEqual([
      "v"
    ]);
  });

  it("groups by org and orders groups by their top sorted row", () => {
    const apps = [
      app({ id: "old", org_slug: "acme", updated_at: "2026-01-01T00:00:00Z" }),
      app({ id: "new", org_slug: "globex", updated_at: "2026-06-01T00:00:00Z" })
    ];
    // Default sort is updated desc → globex (newest) group first.
    const model = buildAppsTableModel(apps, state({ group: "org" }));
    expect(model.groups.map((g) => g.key)).toEqual(["globex", "acme"]);
    expect(model.flatIds).toEqual(["new", "old"]);
  });

  it("keeps flatIds aligned with visual group order for shift-select", () => {
    const apps = [
      app({ id: "a1", org_slug: "acme", name: "A1" }),
      app({ id: "g1", org_slug: "globex", name: "G1" }),
      app({ id: "a2", org_slug: "acme", name: "A2" })
    ];
    const model = buildAppsTableModel(
      apps,
      state({ group: "org", sortKey: "name", sortDir: "asc" })
    );
    // Grouped by org: acme rows contiguous, then globex — flatIds mirrors that.
    expect(model.flatIds).toEqual(["a1", "a2", "g1"]);
  });

  it("sorts by name ascending", () => {
    const apps = [app({ id: "b", name: "Beta" }), app({ id: "a", name: "Alpha" })];
    const model = buildAppsTableModel(
      apps,
      state({ group: "none", sortKey: "name", sortDir: "asc" })
    );
    expect(model.flatIds).toEqual(["a", "b"]);
  });

  it("reports filtered and total counts", () => {
    const apps = [app({ id: "a", name: "keep" }), app({ id: "b", name: "drop" })];
    const model = buildAppsTableModel(apps, state({ q: "keep" }));
    expect(model.filteredCount).toBe(1);
    expect(model.totalCount).toBe(2);
  });
});

describe("fleetStats", () => {
  it("rolls up totals, live/draft, distinct orgs, and source mix", () => {
    const apps = [
      app({ id: "1", org_slug: "acme", source_type: "s3", published_at: "2026-02-01T00:00:00Z" }),
      app({ id: "2", org_slug: "acme", source_type: "s3", published_at: null }),
      app({ id: "3", org_slug: "globex", source_type: "v0", published_at: "2026-02-01T00:00:00Z" }),
      app({ id: "4", org_slug: "globex", source_type: "local", published_at: null })
    ];
    expect(fleetStats(apps)).toEqual({
      total: 4,
      live: 2,
      draft: 2,
      orgs: 2,
      bySource: { s3: 2, v0: 1, local: 1 }
    });
  });

  it("returns an all-zero shape for an empty registry", () => {
    expect(fleetStats([])).toEqual({
      total: 0,
      live: 0,
      draft: 0,
      orgs: 0,
      bySource: { v0: 0, local: 0, s3: 0 }
    });
  });
});
