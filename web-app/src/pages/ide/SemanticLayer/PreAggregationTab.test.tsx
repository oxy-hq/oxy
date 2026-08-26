// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PreaggRollupStatus, PreaggStatusResponse } from "@/services/api/semantic";
import PreAggregationTab from "./PreAggregationTab";

const state = vi.hoisted(() => ({
  value: { data: undefined, isLoading: false, isError: false } as {
    data: PreaggStatusResponse | undefined;
    isLoading: boolean;
    isError: boolean;
  }
}));

vi.mock("@/hooks/api/usePreaggStatus", () => ({ default: () => state.value }));

const rebuildMutate = vi.hoisted(() => vi.fn());
vi.mock("@/hooks/api/useRebuildPreagg", () => ({
  default: () => ({ mutate: rebuildMutate, isPending: false })
}));

const rollup = (over: Partial<PreaggRollupStatus> = {}): PreaggRollupStatus => ({
  view_name: "orders",
  rollup_name: "orders_by_month",
  is_built: true,
  has_parquet: true,
  dimensions: ["order_status"],
  measures: [{ name: "total_orders", measure_type: "count" }],
  time_dimension: "order_date",
  granularity: "month",
  refresh_key: "every 1h",
  build_date: null,
  refresh_key_checked_at: "2026-05-11T14:03:22+00:00",
  empty_since: null,
  ...over
});

const serve = (data: Partial<PreaggStatusResponse>, flags: Partial<typeof state.value> = {}) => {
  state.value = {
    data: { rollups: [], blob_reads_available: true, ...data },
    isLoading: false,
    isError: false,
    ...flags
  };
};

describe("PreAggregationTab", () => {
  beforeEach(() => {
    serve({});
    rebuildMutate.mockClear();
  });
  afterEach(cleanup);

  /**
   * A rebuild that correctly finds zero rows retracts the rollup, so the row
   * goes from Cached to un-built with no build time. Reported as "Not built"
   * it reads as a rebuild that never happened — the row contradicting a run
   * that reported success.
   */
  it("tells an empty rollup apart from one nobody has built", () => {
    serve({
      rollups: [
        rollup({
          rollup_name: "orders_empty",
          is_built: false,
          has_parquet: false,
          build_date: null,
          refresh_key_checked_at: null,
          empty_since: "2026-05-11T14:03:22+00:00"
        }),
        rollup({
          rollup_name: "orders_never",
          is_built: false,
          has_parquet: false,
          build_date: null,
          refresh_key_checked_at: null
        })
      ]
    });
    render(<PreAggregationTab />);
    expect(screen.getByText("Empty")).toBeInTheDocument();
    expect(screen.getByText("Not built")).toBeInTheDocument();
    // And the empty one reports WHEN it emptied rather than "Never" — it did
    // run, and that is the only timestamp the retraction left behind.
    expect(screen.queryAllByText("Never")).toHaveLength(1);
  });

  it("lists every rollup with its cached state", () => {
    serve({
      rollups: [
        rollup(),
        rollup({ rollup_name: "orders_summary", is_built: false, has_parquet: false })
      ]
    });
    render(<PreAggregationTab />);
    expect(screen.getByText("orders_by_month")).toBeInTheDocument();
    expect(screen.getByText("Cached")).toBeInTheDocument();
    expect(screen.getByText("Not built")).toBeInTheDocument();
    expect(screen.getByText(/1 of 2 built/)).toBeInTheDocument();
  });

  it("tells a rollup built on another node apart from one never built", () => {
    // The fleet shape: the manifest is synced everywhere, the Parquet is not.
    // That rollup IS serving queries — this node reads the object from shared
    // storage — so spelling it the same as "never built" reads as a bug and
    // contradicts the Built timestamp on the same row.
    serve({
      rollups: [
        rollup({ rollup_name: "on_this_node" }),
        rollup({ rollup_name: "on_another_node", is_built: true, has_parquet: false }),
        rollup({ rollup_name: "never_built", is_built: false, has_parquet: false })
      ]
    });
    render(<PreAggregationTab />);
    expect(screen.getByText("Cached")).toBeInTheDocument();
    expect(screen.getByText("Built elsewhere")).toBeInTheDocument();
    expect(screen.getByText("Not built")).toBeInTheDocument();
    // And the counter follows "built", not "local" — two of three are serving.
    expect(screen.getByText(/2 of 3 built/)).toBeInTheDocument();
  });

  it("only promises shared storage when the deployment has it", () => {
    // `is_built && !has_parquet` with no blob bucket means the warehouse
    // answers. Saying "Built elsewhere" there would promise a fast path this
    // deployment does not have — the freshness-misreporting class the badge
    // exists to avoid.
    serve({
      rollups: [rollup({ rollup_name: "on_another_node", is_built: true, has_parquet: false })],
      blob_reads_available: false
    });
    render(<PreAggregationTab />);
    expect(screen.getByText("Not cached here")).toBeInTheDocument();
    expect(screen.queryByText("Built elsewhere")).not.toBeInTheDocument();
  });

  it("renders a measure's aggregation type, like the view sidebar", () => {
    serve({ rollups: [rollup()] });
    render(<PreAggregationTab />);
    expect(screen.getByText("total_orders")).toBeInTheDocument();
    expect(screen.getByText("(count)")).toBeInTheDocument();
  });

  it("filters on view, rollup, dimension and measure names", async () => {
    const user = userEvent.setup();
    serve({
      rollups: [
        rollup(),
        rollup({
          view_name: "marketing_spend",
          rollup_name: "spend_by_channel",
          dimensions: ["channel"],
          measures: [{ name: "total_spend", measure_type: "sum" }]
        })
      ]
    });
    render(<PreAggregationTab />);

    await user.type(screen.getByLabelText("Filter rollups"), "channel");
    expect(screen.getByText("spend_by_channel")).toBeInTheDocument();
    expect(screen.queryByText("orders_by_month")).not.toBeInTheDocument();
    // The counter follows the filter, and says what it left out.
    expect(screen.getByText(/1 of 1 built \(2 total\)/)).toBeInTheDocument();
  });

  it("says nothing matched — not that nothing is declared — for a dead filter", async () => {
    const user = userEvent.setup();
    serve({ rollups: [rollup()] });
    render(<PreAggregationTab />);

    await user.type(screen.getByLabelText("Filter rollups"), "zzz");
    expect(screen.getByTestId("pre-aggregation-empty")).toHaveTextContent(/matches this filter/);
  });

  it("lists a declared rollup that has never been built", () => {
    // The reason the list is config-derived: nothing cached is not nothing to
    // show. The row appears, fully described, and says it isn't built.
    serve({
      rollups: [
        rollup({
          is_built: false,
          has_parquet: false,
          refresh_key_checked_at: null,
          build_date: null
        })
      ]
    });
    render(<PreAggregationTab />);
    expect(screen.getByText("orders_by_month")).toBeInTheDocument();
    expect(screen.getByText("Not built")).toBeInTheDocument();
    expect(screen.getByText("Never")).toBeInTheDocument();
    expect(screen.getByText(/0 of 1 built/)).toBeInTheDocument();
  });

  it("renders the declared refresh cadence", () => {
    serve({ rollups: [rollup()] });
    render(<PreAggregationTab />);
    expect(screen.getByText("every 1h")).toBeInTheDocument();
  });

  it("says nothing is declared only when the layer declares nothing", () => {
    serve({ rollups: [] });
    render(<PreAggregationTab />);
    expect(screen.getByTestId("pre-aggregation-empty")).toHaveTextContent(
      /No pre-aggregations declared/
    );
  });

  it("renders an em dash for a build time it cannot parse", () => {
    serve({
      rollups: [rollup({ refresh_key_checked_at: "last tuesday", build_date: null })]
    });
    render(<PreAggregationTab />);
    // Never "Invalid Date" — an old or malformed cache is a normal state.
    expect(screen.queryByText(/Invalid Date/)).not.toBeInTheDocument();
  });

  it("surfaces a failed status read instead of an empty table", () => {
    serve({}, { isError: true, data: undefined });
    render(<PreAggregationTab />);
    expect(screen.getByText(/Could not read the pre-aggregation cache status/)).toBeInTheDocument();
  });

  it("rebuilds one rollup by name", async () => {
    const user = userEvent.setup();
    serve({ rollups: [rollup(), rollup({ rollup_name: "orders_summary" })] });
    render(<PreAggregationTab />);

    await user.click(screen.getByLabelText("Rebuild orders.orders_summary"));
    // The second argument is the per-call `onError` that releases the row's
    // spinner when the submit itself fails; the body is what this asserts.
    expect(rebuildMutate).toHaveBeenCalledWith(
      { view: "orders", rollup: "orders_summary" },
      expect.objectContaining({ onError: expect.any(Function) })
    );
  });

  it("marks only the rollup being rebuilt as in flight", async () => {
    const user = userEvent.setup();
    serve({ rollups: [rollup(), rollup({ rollup_name: "orders_summary" })] });
    render(<PreAggregationTab />);

    await user.click(screen.getByLabelText("Rebuild orders.orders_summary"));
    expect(screen.getByText("Rebuilding…")).toBeInTheDocument();
    // Its neighbour keeps reporting its real state rather than joining in.
    expect(screen.getByText("Cached")).toBeInTheDocument();
  });

  it("stops waiting on a rollup whose rebuild was never accepted", async () => {
    // A 404/503/500 from the submit itself creates no run. Leaving the row
    // spinning would contradict the error toast for the next five minutes and
    // then blame a run history entry that does not exist.
    const user = userEvent.setup();
    serve({ rollups: [rollup({ rollup_name: "orders_summary" })] });
    render(<PreAggregationTab />);

    await user.click(screen.getByLabelText("Rebuild orders.orders_summary"));
    expect(screen.getByText("Rebuilding…")).toBeInTheDocument();

    const onError = rebuildMutate.mock.calls[0][1]?.onError as () => void;
    await act(async () => onError());
    expect(screen.queryByText("Rebuilding…")).not.toBeInTheDocument();
  });

  it("rebuilds everything declared, not just what the filter shows", async () => {
    const user = userEvent.setup();
    serve({
      rollups: [rollup(), rollup({ view_name: "marketing_spend", rollup_name: "spend_by_channel" })]
    });
    render(<PreAggregationTab />);

    await user.type(screen.getByLabelText("Filter rollups"), "marketing");
    await user.click(screen.getByRole("button", { name: /Rebuild all/ }));
    // Empty body is the server's "everything" case — a filtered subset would
    // silently narrow what the button claims to do.
    expect(rebuildMutate).toHaveBeenCalledWith(
      {},
      expect.objectContaining({ onError: expect.any(Function) })
    );
  });
});
