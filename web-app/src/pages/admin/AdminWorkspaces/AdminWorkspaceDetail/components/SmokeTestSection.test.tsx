// @vitest-environment jsdom

import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  WorkspaceHealthSmokeCheck,
  WorkspaceHealthSmokeProbe,
  WorkspaceHealthSmokeProbeKind,
  WorkspaceHealthStatus
} from "@/services/api/workspaceHealth";
import { SmokeTestSection } from "./SmokeTestSection";

afterEach(cleanup);

const check = (
  kind: WorkspaceHealthSmokeProbeKind,
  target: string,
  status: WorkspaceHealthStatus,
  reason: string | null = null,
  duration_ms = 12
): WorkspaceHealthSmokeCheck => ({
  check: `${kind}:${target}`,
  kind,
  target,
  status,
  reason,
  duration_ms
});

/** All four probes, enabled only for the named kinds — mirrors the backend, which
 * always lists every kind with its individual enabled flag. */
const probesWith = (...enabled: WorkspaceHealthSmokeProbeKind[]): WorkspaceHealthSmokeProbe[] =>
  (["connection", "semantic", "app", "agent"] as const).map((kind) => ({
    kind,
    enabled: enabled.includes(kind)
  }));

describe("SmokeTestSection", () => {
  it("summarises probe outcomes, counting cap notes separately from passes", () => {
    render(
      <SmokeTestSection
        checks={[
          check("connection", "bigquery", "healthy"),
          check("semantic", "orders", "unhealthy", "column not found"),
          check("semantic", "returns", "degraded", "probe exceeded its 30s budget"),
          // A healthy check WITH a reason is a cap note, not a passing probe.
          check("semantic", "topics", "healthy", "probed 25 of 30 topics", 0)
        ]}
        probes={probesWith("connection", "semantic")}
        lastRunAt={null}
      />
    );
    expect(screen.getByText("1 passed · 1 failed · 1 degraded · 1 skipped")).toBeInTheDocument();
  });

  it("labels a cap note 'skipped' rather than crediting it as healthy", () => {
    render(
      <SmokeTestSection
        checks={[check("semantic", "topics", "healthy", "probed 25 of 30 topics", 0)]}
        probes={probesWith("semantic")}
        lastRunAt={null}
      />
    );
    expect(screen.getByText("skipped")).toBeInTheDocument();
    expect(screen.queryByText("healthy")).not.toBeInTheDocument();
  });

  it("sorts failures above passes so one bad topic never hides in a long list", () => {
    render(
      <SmokeTestSection
        checks={[
          check("semantic", "aaa_first_alphabetically", "healthy"),
          check("semantic", "zzz_last_alphabetically", "unhealthy", "boom")
        ]}
        probes={probesWith("semantic")}
        lastRunAt={null}
      />
    );
    const targets = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(targets[0]).toContain("zzz_last_alphabetically");
    expect(targets[1]).toContain("aaa_first_alphabetically");
  });

  it("groups by probe kind and reports a per-group pass ratio excluding notes", () => {
    render(
      <SmokeTestSection
        checks={[
          check("connection", "bigquery", "healthy"),
          check("semantic", "orders", "healthy"),
          check("semantic", "returns", "unhealthy", "boom"),
          check("semantic", "topics", "healthy", "probed 2 of 5 topics", 0)
        ]}
        probes={probesWith("connection", "semantic")}
        lastRunAt={null}
      />
    );
    expect(screen.getByText("Connections")).toBeInTheDocument();
    expect(screen.getByText("1/1 passed")).toBeInTheDocument();
    // The cap note must not inflate the denominator.
    expect(screen.getByText("Semantic model")).toBeInTheDocument();
    expect(screen.getByText("1/2 passed")).toBeInTheDocument();
  });

  it("names disabled probe kinds instead of omitting them", () => {
    // The whole point: a workspace running only connections should still SEE that
    // semantic / apps / agent exist and are off, not a blank space.
    render(
      <SmokeTestSection
        checks={[check("connection", "bq", "healthy")]}
        probes={probesWith("connection")}
        lastRunAt={new Date().toISOString()}
      />
    );
    expect(screen.getByText("Connections")).toBeInTheDocument();
    expect(screen.getByText("Semantic model")).toBeInTheDocument();
    expect(screen.getByText("Data apps")).toBeInTheDocument();
    expect(screen.getByText("Agent")).toBeInTheDocument();
    // The three off probes each carry a muted "Not enabled" pill.
    expect(screen.getAllByText("Not enabled")).toHaveLength(3);
  });

  it("distinguishes an enabled-but-empty probe from a not-yet-run one", () => {
    // Enabled, has run (lastRunAt set), but produced no checks → no targets.
    const { unmount } = render(
      <SmokeTestSection
        checks={[]}
        probes={probesWith("connection")}
        lastRunAt={new Date().toISOString()}
      />
    );
    expect(screen.getByText("No targets found")).toBeInTheDocument();
    unmount();

    // Enabled but never run (no lastRunAt) → awaiting first run.
    render(<SmokeTestSection checks={[]} probes={probesWith("connection")} lastRunAt={null} />);
    expect(screen.getByText("Not run yet")).toBeInTheDocument();
  });

  it("omits timing for checks that never ran a probe", () => {
    render(
      <SmokeTestSection
        checks={[
          check("connection", "bigquery", "healthy", null, 1500),
          check("semantic", "topics", "healthy", "skipped 3", 0)
        ]}
        probes={probesWith("connection", "semantic")}
        lastRunAt={null}
      />
    );
    // Sub-second in ms, slower in seconds; a 0ms cap note shows no timing at all.
    expect(screen.getByText("1.5 s")).toBeInTheDocument();
    expect(screen.queryByText("0 ms")).not.toBeInTheDocument();
  });

  it("shows its own last-run time, which lags the rollup's last-checked", () => {
    const { container } = render(
      <SmokeTestSection
        checks={[check("connection", "bq", "healthy")]}
        probes={probesWith("connection")}
        lastRunAt={new Date(Date.now() - 6 * 60 * 60 * 1000).toISOString()}
      />
    );
    expect(within(container).getByText(/Last run/)).toBeInTheDocument();
  });

  it("runs the probes on demand, and locks the button while a run is in flight", async () => {
    const onRun = vi.fn();
    const { rerender } = render(
      <SmokeTestSection
        checks={[check("connection", "bq", "healthy")]}
        probes={probesWith("connection")}
        lastRunAt={null}
        onRun={onRun}
      />
    );
    await userEvent.click(screen.getByRole("button", { name: /run smoke test/i }));
    expect(onRun).toHaveBeenCalledTimes(1);

    // A forced run takes minutes (warehouse round-trips, an LLM call), so the
    // button must be disabled for the whole wait — a second click would enqueue
    // a second eval and bill the probes twice.
    rerender(
      <SmokeTestSection
        checks={[check("connection", "bq", "healthy")]}
        probes={probesWith("connection")}
        lastRunAt={null}
        onRun={onRun}
        isRunning
      />
    );
    const running = screen.getByRole("button", { name: /running/i });
    expect(running).toBeDisabled();
    await userEvent.click(running);
    expect(onRun).toHaveBeenCalledTimes(1);
  });

  it("renders read-only when no run handler is supplied", () => {
    render(
      <SmokeTestSection
        checks={[check("connection", "bq", "healthy")]}
        probes={probesWith("connection")}
        lastRunAt={null}
      />
    );
    expect(screen.queryByRole("button", { name: /run smoke test/i })).not.toBeInTheDocument();
  });

  it("falls back to inferring enabled kinds from checks for pre-smoke_probes rows", () => {
    // Old persisted payloads have verdicts but no `smoke_probes`. Show the kinds
    // that produced checks (as enabled) and omit the rest — the old behaviour.
    render(
      <SmokeTestSection
        checks={[check("connection", "bq", "healthy")]}
        probes={[]}
        lastRunAt={null}
      />
    );
    expect(screen.getByText("Connections")).toBeInTheDocument();
    expect(screen.queryByText("Semantic model")).not.toBeInTheDocument();
    expect(screen.queryByText("Not enabled")).not.toBeInTheDocument();
  });
});
