// @vitest-environment jsdom

/**
 * `/status` is `IdeOnly` — it reads `config.yml` off disk — and this component
 * renders on every NON-IDE page (`WorkspaceShell` hides it only inside the
 * IDE). Re-enabling the query outside the IDE was right, but it made every
 * failure to REACH the ide paint this banner, whose words are about the
 * tenant's configuration.
 *
 * On a fleet with no ide upstream that is a `421` on every page load, forever:
 * a persistent red "your workspace is broken" for a deployment shape. The
 * distinction between "this pod could not reach the files" and "your
 * config.yml is broken" is the one the whole branch is about.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { AxiosError, AxiosHeaders } from "axios";
import type { ReactNode } from "react";
import { createElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import queryKeys from "@/hooks/api/queryKey";
import WorkspaceConfigStatus from "./index";

const mocks = vi.hoisted(() => ({ getWorkspaceStatus: vi.fn() }));

vi.mock("@/hooks/useCurrentWorkspaceBranch", () => ({
  default: () => ({ workspace: { id: "ws-1" }, branchName: "" })
}));
vi.mock("@/services/api/workspaces", () => ({
  WorkspaceService: { getWorkspaceStatus: mocks.getWorkspaceStatus }
}));

const routed = (status: number) => {
  const headers = new AxiosHeaders({ "x-oxy-required-role": "ide" });
  const err = new AxiosError("misdirected", "ERR_BAD_REQUEST", undefined, null, {
    status,
    statusText: "",
    headers,
    // biome-ignore lint/suspicious/noExplicitAny: minimal Axios config stub
    config: { headers } as any,
    data: null
  });
  return err;
};

// Built once per test, NOT inside the wrapper: a client constructed in the
// component body is replaced on every re-render, orphaning the in-flight query.
let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(QueryClientProvider, { client }, children);

describe("WorkspaceConfigStatus", () => {
  beforeEach(() => {
    mocks.getWorkspaceStatus.mockReset();
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  });

  /// Waits for the query to SETTLE in error, so "no banner" is an observation
  /// about a finished render rather than about one that had not started.
  /// Asserting an empty container without this passes while the query is still
  /// pending — which it does even with the guard removed.
  const settled = () =>
    waitFor(() =>
      expect(client.getQueryState(queryKeys.workspaces.status("ws-1", ""))?.status).toBe("error")
    );

  it.each([421, 502])("stays silent when the ide is unreachable (%i)", async (status) => {
    mocks.getWorkspaceStatus.mockImplementation(() => Promise.reject(routed(status)));
    render(createElement(WorkspaceConfigStatus), { wrapper });

    await settled();
    expect(screen.queryByText(/Failed to load workspace status/)).toBeNull();
  });

  it("still reports a real failure to load the status", async () => {
    mocks.getWorkspaceStatus.mockImplementation(() => Promise.reject(new Error("boom")));
    render(createElement(WorkspaceConfigStatus), { wrapper });

    await settled();
    expect(screen.queryByText(/Failed to load workspace status/)).not.toBeNull();
  });
});
