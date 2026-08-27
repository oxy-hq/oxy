// @vitest-environment jsdom

/**
 * The config-status query must not borrow the routing decision.
 *
 * `useCurrentWorkspaceBranch` returns `branchName: ""` outside the IDE on
 * purpose — a non-empty `?branch=` promotes a FleetOk route to IdeOnly. This
 * hook gated `enabled` on that value, and `WorkspaceShell` renders
 * `<WorkspaceStatus />` on every NON-IDE route (`hideStatus = inIde`). So the
 * one surface that tells a user their `config.yml` is broken sat permanently
 * disabled, stuck `isPending`, rendering `null` — everywhere it actually
 * shows.
 *
 * `""` is a valid question, not a missing one: `getWorkspaceStatus` omits the
 * param when falsy and the server reads that as the default branch
 * (`normalize_branch_hint`).
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { createElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useWorkspaceStatus } from "./useWorkspaceStatus";

const mocks = vi.hoisted(() => ({
  branchName: "",
  workspaceId: "ws-1",
  getWorkspaceStatus: vi.fn()
}));

vi.mock("@/hooks/useCurrentWorkspaceBranch", () => ({
  default: () => ({
    workspace: { id: mocks.workspaceId },
    branchName: mocks.branchName
  })
}));

vi.mock("@/services/api/workspaces", () => ({
  WorkspaceService: { getWorkspaceStatus: mocks.getWorkspaceStatus }
}));

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(
    QueryClientProvider,
    { client: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
    children
  );

describe("useWorkspaceStatus", () => {
  beforeEach(() => {
    mocks.branchName = "";
    mocks.workspaceId = "ws-1";
    mocks.getWorkspaceStatus.mockReset();
    mocks.getWorkspaceStatus.mockResolvedValue({ is_valid: true });
  });

  it("runs outside the IDE, where the branch is empty by design", async () => {
    const { result } = renderHook(() => useWorkspaceStatus(), { wrapper });

    await waitFor(() => expect(result.current.isPending).toBe(false));
    expect(mocks.getWorkspaceStatus).toHaveBeenCalledWith("ws-1", "");
  });

  it("still runs inside the IDE, where a branch is selected", async () => {
    mocks.branchName = "feature-x";
    const { result } = renderHook(() => useWorkspaceStatus(), { wrapper });

    await waitFor(() => expect(result.current.isPending).toBe(false));
    expect(mocks.getWorkspaceStatus).toHaveBeenCalledWith("ws-1", "feature-x");
  });

  it("stays disabled without a workspace — the one thing it cannot ask without", () => {
    mocks.workspaceId = "";
    renderHook(() => useWorkspaceStatus(), { wrapper });
    expect(mocks.getWorkspaceStatus).not.toHaveBeenCalled();
  });
});
