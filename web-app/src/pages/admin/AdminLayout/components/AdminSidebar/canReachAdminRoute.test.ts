// @vitest-environment jsdom
// Needed only because this module is reached through the sidebar component, which pulls
// in `services/env` and reads `window.location`. The function under test touches neither.
import { describe, expect, it } from "vitest";
import ROUTES from "@/libs/utils/routes";
import type { PlatformCapability } from "@/types/auth";
import { canReachAdminRoute, firstReachableAdminRoute } from "./index";

/**
 * The route guard, tested by **behaviour on real pathnames** rather than by inspecting
 * the map.
 *
 * Both bugs this file exists for were invisible to a shape check. The first version used
 * a bare `startsWith` under a comment claiming segment-boundary matching. The second kept
 * the query string in `i.to`, so no tenant entry could ever match a `location.pathname`
 * and the guard silently returned `true` for the largest group in the map — it "worked"
 * in the sense that nobody was wrongly bounced, which is exactly why nothing noticed.
 */

const staff = (...capabilities: PlatformCapability[]) => ({ isOwner: false, capabilities });
const owner = { isOwner: true, capabilities: [] as PlatformCapability[] };
const nobody = staff();

// Paths come from the route table, never typed out here: an invented path matches no
// entry and the guard admits unknown paths, so a typo turns every assertion in its test
// into a vacuous `expect(true).toBe(true)`. `/admin/billing-queue` was such a typo — the
// real constant is `/admin/billing/queue`.

describe("canReachAdminRoute", () => {
  it("admits the tenants directory to any ONE of its three capabilities", () => {
    // Organizations / Partners / Users are three entries at one path, each naming a
    // different capability. Longest-match-then-apply would have bounced the second and
    // third off a page they can plainly use.
    expect(canReachAdminRoute("/admin/tenants", staff("manage_org_settings"))).toBe(true);
    expect(canReachAdminRoute("/admin/tenants", staff("manage_partners"))).toBe(true);
    expect(canReachAdminRoute("/admin/tenants", staff("manage_members"))).toBe(true);
  });

  it("refuses the tenants directory to staff holding none of them", () => {
    // The assertion the query-string bug made unreachable: with `?type=orgs` left on
    // `i.to`, nothing matched and this returned true.
    expect(canReachAdminRoute("/admin/tenants", staff("manage_apps"))).toBe(false);
  });

  it("gates an owner-only room on the boolean, not a capability", () => {
    expect(canReachAdminRoute(ROUTES.ADMIN.BILLING_QUEUE, owner)).toBe(true);
    // Holding every capability is still not being root.
    expect(
      canReachAdminRoute(
        ROUTES.ADMIN.BILLING_QUEUE,
        staff("manage_platform_grants", "operate_platform", "view_tenants")
      )
    ).toBe(false);
  });

  it("gates airway on operate_platform, matching the endpoint", () => {
    // Regression: this entry was `ownerOnly` while the server mounted
    // `airway_config` under `cap(Action::PlatformOperate)`. A holder of that
    // capability got a 200 from `GET /admin/airway/config` and was still
    // bounced off the page — the nav and the API disagreeing about one
    // surface, which is the bug this asserts against.
    expect(canReachAdminRoute(ROUTES.ADMIN.AIRWAY, staff("operate_platform"))).toBe(true);
    expect(canReachAdminRoute(ROUTES.ADMIN.AIRWAY, owner)).toBe(true);
    // Still staff-only: the capability is the door, not the absence of one.
    expect(canReachAdminRoute(ROUTES.ADMIN.AIRWAY, staff("manage_apps"))).toBe(false);
    expect(canReachAdminRoute(ROUTES.ADMIN.AIRWAY, nobody)).toBe(false);
  });

  it("gates OLTP on operate_platform, matching the endpoint", () => {
    // The nav and the route gate must name the same capability. The server
    // mounts these routes under `cap(Action::PlatformOltp)`, which resolves to
    // `operate_platform` — deliberately NOT `manage_apps`, because provisioning
    // creates a billable database and an App Operator ships apps and nothing
    // else. An App Operator seeing the entry and getting a 403 would be the
    // same nav/API disagreement the airway case above regressed on.
    expect(canReachAdminRoute(ROUTES.ADMIN.OLTP, staff("operate_platform"))).toBe(true);
    expect(canReachAdminRoute(ROUTES.ADMIN.OLTP, owner)).toBe(true);
    expect(canReachAdminRoute(ROUTES.ADMIN.OLTP, staff("manage_apps"))).toBe(false);
    expect(canReachAdminRoute(ROUTES.ADMIN.OLTP, nobody)).toBe(false);
  });

  it("gates the grant console on manage_platform_grants", () => {
    expect(canReachAdminRoute("/admin/app-admins", staff("manage_platform_grants"))).toBe(true);
    expect(canReachAdminRoute("/admin/app-admins", staff("manage_apps"))).toBe(false);
    expect(canReachAdminRoute("/admin/app-admins", owner)).toBe(true);
  });

  it("lets a nested route inherit its parent's rule", () => {
    expect(canReachAdminRoute("/admin/apps/some-app-id", staff("manage_apps"))).toBe(true);
    expect(canReachAdminRoute("/admin/apps/some-app-id", nobody)).toBe(false);
  });

  it("does not let a sibling with a shared prefix inherit that rule", () => {
    // `/admin/apps-registry` is not under `/admin/apps`. A bare `startsWith` says it is,
    // which is what the comment claimed was already handled.
    expect(canReachAdminRoute("/admin/apps-registry", nobody)).toBe(true);
    // And the real neighbour that shares four characters stays on its own rule.
    expect(canReachAdminRoute("/admin/app-admins", staff("manage_apps"))).toBe(false);
  });

  it("admits an unknown path — this is a stale-bookmark redirect, not a control", () => {
    expect(canReachAdminRoute("/admin/not-in-the-nav", nobody)).toBe(true);
  });
});

/**
 * The bounce target must be somewhere the same principal can be.
 *
 * `AdminLayout` sent everyone to Custom apps, which is gated on `manage_apps`. Every role
 * shipping today holds it, so nothing was broken — but the premise of this branch is that
 * a narrower preset is now cheap, and the first one omitting `manage_apps` turns the
 * bounce into a redirect cycle. A cycle is not a wrong answer the guard can report; it is
 * a hung page.
 */
describe("firstReachableAdminRoute", () => {
  it("never returns a route the same standing cannot reach", () => {
    const standings = [
      owner,
      staff("manage_apps", "develop_apps"), // App Operator
      staff("view_audit"), // an audit-only preset that does not exist yet
      staff("manage_platform_grants"), // a grants-only preset that does not exist yet
      staff("manage_members")
    ];
    for (const standing of standings) {
      const target = firstReachableAdminRoute(standing);
      // The two functions take different things and the difference is load-bearing:
      // this returns a `to` (a link target, query included — the tenants directory needs
      // `?type=`), while the guard takes a `location.pathname`, which never has one.
      // React Router does this strip for us at runtime: `<Navigate to="/admin/tenants
      // ?type=users">` lands with `pathname === "/admin/tenants"`.
      //
      // Passing the raw `to` here made the `manage_members` case — the ONLY one that
      // reaches a tenants entry, and so the only one covering the any-of rule — match no
      // candidate and pass through the unknown-path fallback. It asserted nothing, and
      // would have kept passing if that target became genuinely unreachable.
      const landedPath = target.split("?")[0];
      expect(
        canReachAdminRoute(landedPath, standing),
        `bounce target ${target} is itself unreachable — that is a redirect cycle`
      ).toBe(true);
    }
  });

  it("sends a principal with no admin surface home rather than into the console", () => {
    expect(firstReachableAdminRoute(nobody)).toBe("/");
  });
});
