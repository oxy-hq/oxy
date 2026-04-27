// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DatabaseSchema } from "@/types/database";
import useDatabaseSchema from "./useDatabaseSchema";

// ── Module mocks ──────────────────────────────────────────────────────────────

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "proj-1" }, branchName: "main" })
}));

const mockGetDatabaseSchema = vi.fn();
vi.mock("@/services/api", () => ({
  DatabaseService: {
    getDatabaseSchema: (...args: unknown[]) => mockGetDatabaseSchema(...args)
  }
}));

// ── Helpers ───────────────────────────────────────────────────────────────────

const mockSchema: DatabaseSchema = {
  tables: [
    { name: "users", columns: [{ name: "id", data_type: "int4" }] },
    { name: "orders", columns: [{ name: "total", data_type: "numeric" }] }
  ]
};

function makeWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client }, children);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

beforeEach(() => {
  mockGetDatabaseSchema.mockReset();
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("useDatabaseSchema", () => {
  it("does not fetch when enabled=false", () => {
    renderHook(() => useDatabaseSchema("my-db", false), { wrapper: makeWrapper() });
    expect(mockGetDatabaseSchema).not.toHaveBeenCalled();
  });

  it("fetches when enabled=true", async () => {
    mockGetDatabaseSchema.mockResolvedValue(mockSchema);
    const { result } = renderHook(() => useDatabaseSchema("my-db", true), {
      wrapper: makeWrapper()
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mockGetDatabaseSchema).toHaveBeenCalledWith("proj-1", "main", "my-db");
  });

  it("returns schema data on success", async () => {
    mockGetDatabaseSchema.mockResolvedValue(mockSchema);
    const { result } = renderHook(() => useDatabaseSchema("my-db", true), {
      wrapper: makeWrapper()
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(mockSchema);
  });

  it("returns isError on API failure", async () => {
    mockGetDatabaseSchema.mockRejectedValue(new Error("connection refused"));
    const { result } = renderHook(() => useDatabaseSchema("my-db", true), {
      wrapper: makeWrapper()
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
  });

  it("transitions from loading to success", async () => {
    mockGetDatabaseSchema.mockResolvedValue(mockSchema);
    const { result } = renderHook(() => useDatabaseSchema("my-db", true), {
      wrapper: makeWrapper()
    });
    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.data).toEqual(mockSchema);
  });
});
