// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PostLoginDispatcher from ".";

const orgs = vi.hoisted(() => ({ data: [] as { id: string; slug: string }[] }));
const workspaces = vi.hoisted(() => ({ data: [] as { id: string; status: string }[] }));

vi.mock("@/hooks/api/organizations", () => ({
  useOrgs: () => ({ data: orgs.data, isPending: false, isError: false })
}));
vi.mock("@/hooks/api/users/useCurrentUser", () => ({
  default: () => ({ data: { partner_memberships: [], is_app_admin: false } })
}));
vi.mock("@/hooks/api/workspaces/useWorkspaces", () => ({
  useAllWorkspaces: () => ({ data: workspaces.data, isPending: false, isError: false })
}));
vi.mock("@/libs/orgSubdomain", () => ({ getInjectedOrg: () => null }));
const appCounts = vi.hoisted(() => ({ data: {} as Record<string, number> }));
vi.mock("@/hooks/api/customApps/useWorkspacesCustomApps", () => ({
  useWorkspacesCustomApps: (ids: readonly string[]) => ({
    appsByWorkspace: new Map(
      ids.map((id) => [id, Array.from({ length: appCounts.data[id] ?? 0 })])
    ),
    isPending: false
  })
}));

function Landed() {
  const location = useLocation();
  return <p data-testid='landed'>{`${location.pathname}${location.search}`}</p>;
}

const land = (url: string) => {
  render(
    <MemoryRouter initialEntries={[url]}>
      <Routes>
        <Route index element={<PostLoginDispatcher />} />
        <Route path='*' element={<Landed />} />
      </Routes>
    </MemoryRouter>
  );
  return screen.getByTestId("landed").textContent ?? "";
};

describe("PostLoginDispatcher", () => {
  afterEach(cleanup);

  beforeEach(() => {
    localStorage.clear();
    orgs.data = [{ id: "org-1", slug: "acme" }];
    workspaces.data = [{ id: "ws-1", status: "ready" }];
    appCounts.data = {};
  });

  it("carries the token emails' ?settings= link into the workspace it picks", () => {
    const landed = land("/?settings=account.tokens");
    expect(landed).toContain("acme");
    expect(landed).toContain("ws-1");
    expect(landed.endsWith("?settings=account.tokens")).toBe(true);
  });

  it("carries it to the org root when there is no workspace yet", () => {
    workspaces.data = [];
    const landed = land("/?settings=organization.api_access");
    expect(landed).toContain("acme");
    expect(landed.endsWith("?settings=organization.api_access")).toBe(true);
  });

  it("forwards nothing else", () => {
    expect(land("/?utm=mail")).not.toContain("?");
  });

  it("lands a first visit on the workspace with the most apps", () => {
    workspaces.data = [
      { id: "ws-empty", status: "ready" },
      { id: "ws-apps", status: "ready" }
    ];
    appCounts.data = { "ws-apps": 2 };
    expect(land("/")).toBe("/acme/workspaces/ws-apps");
  });
});
