// @vitest-environment jsdom

/**
 * `branchName` is a request-routing decision, not just a label.
 *
 * `role_middleware::escalate_for_branch` promotes ANY FleetOk route carrying a
 * non-empty `?branch=` to IdeOnly, so it is reverse-proxied to the single node
 * that owns the workspace files. Outside the IDE this hook's `selectedBranch`
 * is `active_branch` — whatever the working copy happens to be checked out on,
 * normally "main" — and every service that takes a branch attaches it. The
 * result was that /apps, /agents, /databases and friends went to the singleton
 * on every ordinary page load, which is exactly what the compile boundary
 * exists to stop.
 *
 * Empty rather than absent because that is the server's existing contract:
 * `normalize_branch_hint` filters `""` to `None`, pinned by
 * `normalize_branch_hint_strips_empty`.
 */

import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  insideIDE: false,
  workspace: {
    id: "ws-1",
    active_branch: { name: "main" },
    default_branch: "main",
    protected_branches: ["main"],
    capabilities: undefined
  } as Record<string, unknown> | undefined,
  ideBranch: undefined as string | undefined
}));

vi.mock("@/pages/ide", () => ({ useIDE: () => ({ insideIDE: mocks.insideIDE }) }));
vi.mock("@/stores/useCurrentWorkspace", () => ({
  default: () => ({ workspace: mocks.workspace })
}));
vi.mock("@/stores/useIdeBranch", () => ({
  default: () => ({ getCurrentBranch: () => mocks.ideBranch })
}));

import useCurrentWorkspaceBranch from "./useCurrentWorkspaceBranch";

describe("useCurrentWorkspaceBranch", () => {
  beforeEach(() => {
    mocks.insideIDE = false;
    mocks.ideBranch = undefined;
    mocks.workspace = {
      id: "ws-1",
      active_branch: { name: "main" },
      default_branch: "main",
      protected_branches: ["main"],
      capabilities: undefined
    };
  });

  it("sends no branch outside the IDE, even when the working copy is on main", () => {
    const { result } = renderHook(() => useCurrentWorkspaceBranch());
    expect(result.current.branchName).toBe("");
  });

  it("sends no branch outside the IDE when the working copy is on a feature branch", () => {
    // The surface still reads the promoted revision. A working copy parked on
    // a feature branch is the IDE's business, and on a replica there is no
    // working copy to follow anyway.
    mocks.workspace = { ...mocks.workspace, active_branch: { name: "feat/x" } };
    const { result } = renderHook(() => useCurrentWorkspaceBranch());
    expect(result.current.branchName).toBe("");
  });

  it("sends the IDE's selected branch inside the IDE", () => {
    mocks.insideIDE = true;
    mocks.ideBranch = "feat/x";
    const { result } = renderHook(() => useCurrentWorkspaceBranch());
    expect(result.current.branchName).toBe("feat/x");
  });

  it("falls back to the checked-out branch inside the IDE with no selection", () => {
    mocks.insideIDE = true;
    const { result } = renderHook(() => useCurrentWorkspaceBranch());
    expect(result.current.branchName).toBe("main");
  });
});
