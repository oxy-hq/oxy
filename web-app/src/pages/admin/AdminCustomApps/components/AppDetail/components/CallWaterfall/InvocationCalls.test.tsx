// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { InvocationCalls as Trace } from "@/types/apps";
import { InvocationCalls } from "./InvocationCalls";

type Answer = { isPending?: boolean; isError?: boolean; error?: unknown; data?: Trace };
let answer: Answer = {};

vi.mock("@/hooks/api/customApps/useAppFunctions", () => ({
  useInvocationCalls: () => ({ isPending: false, isError: false, refetch: vi.fn(), ...answer })
}));

afterEach(() => {
  cleanup();
  answer = {};
});

const trace = (over: Partial<Trace>): Trace => ({
  invocation_id: "iv-1",
  function: "store",
  environment: "production",
  build_id: "r4",
  mode: "route",
  status: "success",
  failed: false,
  duration_ms: 400,
  created_at: "2026-10-21T10:00:00Z",
  recorded: true,
  reason: null,
  init_ms: 30,
  total_calls: 0,
  dropped: 0,
  unfinished: 0,
  retention_days: 3,
  calls: [],
  ...over
});

const show = (a: Answer) => {
  answer = a;
  render(<InvocationCalls appId='app-1' invocationId='iv-1' />);
};

describe("InvocationCalls", () => {
  it("shows a failed fetch as the read's error, never as no calls", () => {
    show({ isError: true, error: new Error("calls route failed") });
    expect(screen.getByTestId("admin-async-error")).toHaveTextContent("calls route failed");
    expect(screen.queryByTestId("admin-app-invocation-calls-empty")).not.toBeInTheDocument();
  });

  it("tells a run that made no calls from one with no trace", () => {
    show({ data: trace({ total_calls: 0, calls: [] }) });
    expect(screen.getByTestId("admin-app-invocation-calls-empty")).toHaveTextContent(
      "Made no ctx calls."
    );
    expect(screen.queryByTestId("admin-app-invocation-calls-unrecorded")).not.toBeInTheDocument();
  });

  it("says an expired trace has expired, with the retention", () => {
    show({ data: trace({ recorded: false, reason: "expired", init_ms: null }) });
    const note = screen.getByTestId("admin-app-invocation-calls-unrecorded");
    expect(note).toHaveTextContent("The call trace has expired: traces are kept for 3 days.");
    expect(screen.queryByTestId("admin-app-invocation-calls-empty")).not.toBeInTheDocument();
  });

  it("draws one bar per call with its op, after the isolate start", () => {
    show({
      data: trace({
        total_calls: 3,
        calls: [
          {
            seq: 0,
            op: "query",
            family: "query",
            start_ms: 31,
            duration_ms: 120,
            status: "ok",
            verb: "SELECT",
            target: "oxymart",
            rows: 1
          },
          {
            seq: 1,
            op: "storage.list",
            family: "storage",
            start_ms: 31,
            duration_ms: 4,
            status: "error",
            error_type: "host_call_failed"
          },
          {
            seq: 2,
            op: "query",
            family: "query",
            start_ms: 152,
            duration_ms: 90,
            status: "ok",
            verb: "SELECT",
            target: "oxymart",
            rows: 2
          }
        ]
      })
    });
    const rows = screen.getAllByTestId("admin-app-invocation-calls-row");
    expect(rows.map((r) => r.getAttribute("data-op"))).toEqual(["query", "storage.list", "query"]);
    expect(rows[0]).toHaveTextContent("SELECT oxymart · 1 row");
    expect(rows[1]).toHaveTextContent("✕ host_call_failed");
    expect(screen.getByTestId("admin-app-invocation-calls-isolate")).toBeInTheDocument();
    expect(screen.queryByTestId("admin-app-invocation-calls-truncated")).not.toBeInTheDocument();
  });

  it("says when the cap cut the trace", () => {
    show({
      data: trace({
        total_calls: 250,
        dropped: 50,
        calls: Array.from({ length: 200 }, (_, seq) => ({
          seq,
          op: "query",
          family: "query",
          start_ms: seq,
          duration_ms: 1,
          status: "ok" as const
        }))
      })
    });
    expect(screen.getByTestId("admin-app-invocation-calls-truncated")).toHaveTextContent(
      "Showing the first 200 calls to start of 250."
    );
  });
});
