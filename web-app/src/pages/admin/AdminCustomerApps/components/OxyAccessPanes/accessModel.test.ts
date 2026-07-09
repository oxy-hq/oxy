import { describe, expect, it } from "vitest";
import type { AdminOrgMeta } from "@/services/api/adminTenants";
import type { OxyAccessGrant } from "@/types/apps";
import { buildAccessOrgs } from "./accessModel";

const org = (over: Partial<AdminOrgMeta>): AdminOrgMeta => ({
  id: "o",
  name: "Org",
  slug: "org",
  created_at: "2026-01-01T00:00:00Z",
  member_count: 0,
  workspace_count: 0,
  owner_email: null,
  ...over
});

const grant = (over: Partial<OxyAccessGrant>): OxyAccessGrant => ({
  workspace_id: "w",
  workspace_name: "WS",
  org_id: "o",
  org_name: "Org",
  org_slug: "org",
  granted_by_email: null,
  granted_at: "2026-01-01T00:00:00Z",
  ...over
});

describe("buildAccessOrgs", () => {
  it("flags granted vs no-access orgs and sorts by name", () => {
    const orgs = [
      org({ id: "b", name: "Beta", slug: "beta", workspace_count: 3 }),
      org({ id: "a", name: "Alpha", slug: "alpha", workspace_count: 2 })
    ];
    const grants = [
      grant({ org_id: "a", org_slug: "alpha", org_name: "Alpha", workspace_id: "w1" })
    ];

    const result = buildAccessOrgs(orgs, grants);
    expect(result.map((o) => o.orgId)).toEqual(["a", "b"]); // Alpha before Beta

    const a = result.find((o) => o.orgId === "a");
    expect(a?.hasAccess).toBe(true);
    expect(a?.grants).toHaveLength(1);

    const b = result.find((o) => o.orgId === "b");
    expect(b?.hasAccess).toBe(false);
    expect(b?.grants).toHaveLength(0);
    expect(b?.workspaceCount).toBe(3);
  });

  it("unions in a granted org missing from the admin directory", () => {
    const result = buildAccessOrgs([], [grant({ org_id: "x", org_slug: "x", org_name: "X" })]);
    expect(result).toHaveLength(1);
    expect(result[0].hasAccess).toBe(true);
    expect(result[0].workspaceCount).toBe(0);
  });

  it("returns an empty list when there are no orgs or grants", () => {
    expect(buildAccessOrgs([], [])).toEqual([]);
  });
});
