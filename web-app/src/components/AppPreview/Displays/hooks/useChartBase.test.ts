// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── module mocks ──────────────────────────────────────────────────────────────

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "proj-1" }, branchName: "main" })
}));

vi.mock("@/stores/useTheme", () => ({
  default: () => ({ theme: "light" })
}));

const mockClose = vi.fn().mockResolvedValue(undefined);
const mockConnect = vi.fn().mockResolvedValue({ close: mockClose });
const mockDb = { connect: mockConnect };
const mockGetDuckDB = vi.fn().mockResolvedValue(mockDb);

vi.mock("@/libs/duckdb", () => ({
  getDuckDB: () => mockGetDuckDB()
}));

const mockGetData = vi.fn();
const mockRegisterFromTableData = vi.fn();

vi.mock("../utils", () => ({
  getData: (...args: unknown[]) => mockGetData(...args),
  registerFromTableData: (...args: unknown[]) => mockRegisterFromTableData(...args)
}));

// ── helpers ───────────────────────────────────────────────────────────────────

import type { DataContainer } from "@/types/app";
import type { BaseChartDisplay, ChartOptionsBuilder } from "./types";
import { useChartBase } from "./useChartBase";

const makeDisplay = (data = "result"): BaseChartDisplay => ({ data, title: "Test" });
const makeTableData = () => ({ file_path: "/tmp/data.parquet", json: '[{"x":1}]' });
const makeData = (): DataContainer => makeTableData();

/** Wraps renderHook with a fresh QueryClient so tests don't share cache. */
function makeWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } }
  });
  return ({ children }: { children: React.ReactNode }) =>
    createElement(QueryClientProvider, { client: queryClient }, children);
}

/** Creates a builder whose resolution is under manual control. */
function deferredBuilder() {
  let resolve: (v: object) => void = () => {};
  let reject: (e: unknown) => void = () => {};
  const promise = new Promise<object>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockReturnValue(promise);
  return { builder, resolve, reject };
}

// ── setup / teardown ──────────────────────────────────────────────────────────

beforeEach(() => {
  mockGetDuckDB.mockResolvedValue(mockDb);
  mockConnect.mockResolvedValue({ close: mockClose });
  mockClose.mockResolvedValue(undefined);
  mockGetData.mockReturnValue(makeTableData());
  mockRegisterFromTableData.mockResolvedValue("file.parquet");
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ── tests ─────────────────────────────────────────────────────────────────────

describe("useChartBase – isLoading lifecycle", () => {
  it("starts with isLoading = true", () => {
    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    expect(result.current.isLoading).toBe(true);
  });

  it("sets isLoading to false after successful chart load", async () => {
    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it("sets isLoading to false when data is undefined (no-data path)", async () => {
    const display = makeDisplay();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data: undefined, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it("sets isLoading to false when getData returns null", async () => {
    mockGetData.mockReturnValue(null);

    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it("sets isLoading to false when json is an empty array", async () => {
    mockGetData.mockReturnValue({ file_path: "/tmp/data.parquet", json: "[]" });

    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it("sets isLoading to false when buildChartOptions throws", async () => {
    const display = makeDisplay();
    const data = makeData();
    const errorBuilder: ChartOptionsBuilder<BaseChartDisplay> = vi
      .fn()
      .mockRejectedValue(new Error("DuckDB failure"));

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: errorBuilder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it("sets isLoading to false when registerFromTableData throws", async () => {
    mockRegisterFromTableData.mockRejectedValue(new Error("Registration failed"));

    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it("sets isLoading to false when getDuckDB throws", async () => {
    mockGetDuckDB.mockRejectedValue(new Error("DuckDB unavailable"));

    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  /**
   * Regression: race condition on the CDN init path in duckdb.ts.
   *
   * The CDN branch assigns the module-level `duckDB` variable synchronously
   * before `instantiate()` resolves.  A concurrent `getDuckDB()` call sees a
   * truthy `duckDB`, skips `await init()`, and returns the uninstantiated
   * instance.  Calling `connect()` on that instance throws
   * "DuckDB not initialized".
   *
   * The hook must treat this as an error (not get stuck in loading) and
   * surface `createErrorOptions` instead of throwing to the caller.
   */
  it("shows error state when getDuckDB resolves but connect throws 'DuckDB not initialized'", async () => {
    mockConnect.mockRejectedValue(new Error("DuckDB not initialized"));

    const display = makeDisplay();
    const data = makeData();
    const builder: ChartOptionsBuilder<BaseChartDisplay> = vi.fn().mockResolvedValue({});

    const { result } = renderHook(
      () => useChartBase({ display, data, buildChartOptions: builder }),
      { wrapper: makeWrapper() }
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    // The hook must not be stuck and must surface the error chart options.
    expect(result.current.chartOptions).toMatchObject({
      graphic: expect.arrayContaining([
        expect.objectContaining({ style: expect.objectContaining({ text: "Error loading chart" }) })
      ])
    });
  });

  /**
   * Race-condition test: when display changes mid-flight, the stale query must
   * NOT affect the new query's loading state. React Query handles this naturally
   * since each unique query key has independent state — the stale query's result
   * lands in its own cache entry, leaving the new query's isPending unaffected.
   */
  it("does NOT set isLoading to false when a stale query resolves before the current one", async () => {
    const first = deferredBuilder();
    const second = deferredBuilder();

    const display1 = makeDisplay("result-1");
    const display2 = makeDisplay("result-2");
    const data = makeData();

    const { result, rerender } = renderHook(
      ({ display }: { display: BaseChartDisplay }) => {
        const builder = display.data === "result-1" ? first.builder : second.builder;
        return useChartBase({ display, data, buildChartOptions: builder });
      },
      { initialProps: { display: display1 }, wrapper: makeWrapper() }
    );

    // Query for display1 is in-flight.
    expect(result.current.isLoading).toBe(true);

    // Switch to display2 → React Query starts a new independent query.
    rerender({ display: display2 });

    // Resolve the FIRST (stale) query.
    await act(async () => {
      first.resolve({});
    });

    // The stale query's result lands in the display1 cache slot.
    // The hook is now rendering display2's query, which is still pending.
    expect(result.current.isLoading).toBe(true);

    // Resolve the current query → isLoading should now be false.
    await act(async () => {
      second.resolve({});
    });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });
});
