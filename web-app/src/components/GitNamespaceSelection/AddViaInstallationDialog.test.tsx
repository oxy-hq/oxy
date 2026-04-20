// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import AddViaInstallationDialog from "./AddViaInstallationDialog";

vi.mock("@/hooks/api/github/useGitHubAccount", () => ({
  useGitHubAccount: () => ({ data: { connected: false } })
}));
vi.mock("@/hooks/api/github/useUserInstallations", () => ({
  useUserInstallations: () => ({ data: [], isLoading: false, isError: false })
}));
vi.mock("@/hooks/api/github/useCreateInstallationNamespace", () => ({
  useCreateInstallationNamespace: () => ({ mutateAsync: vi.fn(), isPending: false })
}));
vi.mock("@/hooks/api/github/useConnectGitHubAccount", () => ({
  useConnectGitHubAccount: () => ({ mutate: vi.fn(), isPending: false })
}));

afterEach(() => cleanup());

const wrap = (ui: React.ReactNode) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{ui}</QueryClientProvider>;
};

describe("AddViaInstallationDialog", () => {
  it("shows Connect GitHub step when account not connected", () => {
    render(
      wrap(
        <AddViaInstallationDialog
          orgId='org-1'
          open={true}
          onClose={() => {}}
          onConnected={() => {}}
        />
      )
    );
    expect(screen.getByRole("button", { name: /connect github/i })).toBeInTheDocument();
  });
});
