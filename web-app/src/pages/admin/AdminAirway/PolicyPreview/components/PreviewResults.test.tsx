// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type {
  AirwayPolicyPreviewResponse,
  AirwayResourceVerdict,
  AirwayUnevaluatedPipeline
} from "@/services/api/airwayConfig";
import { PreviewResults } from "./PreviewResults";

/**
 * Two claims this component makes about a scan, both of which have been wrong:
 *
 * - **the denominator.** "N of M scanned pipelines" was built from `resources`
 *   alone, so a pipeline that could not be built — which produces no resources
 *   at all — was missing from M. One `not_fixable_here` pipeline beside one
 *   `unevaluated` one rendered as "halts every pipeline scanned".
 * - **what the scan covered.** The endpoint is now fenced to the caller's
 *   platform scope, so the verdict list describes only the tenants they reach
 *   while the *global* policy they can save reaches every tenant. Saying
 *   nothing would make a fleet-wide change read as a small, clean one.
 */

afterEach(cleanup);

const WS = "11111111-1111-1111-1111-111111111111";

function verdict(overrides: Partial<AirwayResourceVerdict> = {}): AirwayResourceVerdict {
  return {
    pipeline_ref: `${WS}:pipelines/toast.airway.yml`,
    resource: "orders",
    mutability: "undeclared",
    passes: true,
    reason: null,
    not_fixable_here: false,
    ...overrides
  };
}

function unevaluated(path: string): AirwayUnevaluatedPipeline {
  return {
    pipeline_ref: `${WS}:pipelines/${path}`,
    error: "connector could not be built: invalid toast config"
  };
}

function body(overrides: Partial<AirwayPolicyPreviewResponse> = {}): AirwayPolicyPreviewResponse {
  return {
    source_kind: "toast",
    contract_policy: "permissive",
    environment: "production",
    resources: [],
    unevaluated: [],
    uncompiled_workspaces: 0,
    out_of_scope_pipelines: 0,
    ...overrides
  };
}

const renderResults = (data: AirwayPolicyPreviewResponse) =>
  render(<PreviewResults data={data} sourceKind='toast' />);

describe("PreviewResults — the not-fixable denominator", () => {
  it("counts a pipeline that could not be evaluated as scanned", () => {
    renderResults(
      body({
        resources: [verdict({ passes: false, not_fixable_here: true })],
        unevaluated: [unevaluated("broken.airway.yml")]
      })
    );

    const banner = screen.getByTestId("admin-airway-not-fixable-banner-toast");
    // 1 of 2 — the unevaluated pipeline was scanned, just not scored. Counting
    // `resources` alone made this "every toast pipeline scanned".
    expect(banner).toHaveTextContent("1 of 2");
    expect(banner).not.toHaveTextContent("every");
  });

  it("still says 'every' when the upstream gap really does cover every pipeline", () => {
    renderResults(
      body({
        resources: [
          verdict({ passes: false, not_fixable_here: true }),
          verdict({
            pipeline_ref: `${WS}:pipelines/other.airway.yml`,
            passes: false,
            not_fixable_here: true
          })
        ]
      })
    );

    expect(screen.getByTestId("admin-airway-not-fixable-banner-toast")).toHaveTextContent(
      "every toast pipeline scanned (2)"
    );
  });
});

describe("PreviewResults — pipelines the caller's scope withheld", () => {
  it("reports the withheld count and says it was not scored", () => {
    renderResults(body({ resources: [verdict()], out_of_scope_pipelines: 137 }));

    const note = screen.getByTestId("admin-airway-out-of-scope-toast");
    expect(note).toHaveTextContent("Not scored: 137 compiled pipelines");
    // The reason it matters: the global row this operator can still save is
    // fleet-wide.
    expect(note).toHaveTextContent("Saving the global toast policy still applies to them");
  });

  it("says nothing at all for an unbounded caller", () => {
    renderResults(body({ resources: [verdict()] }));
    expect(screen.queryByTestId("admin-airway-out-of-scope-toast")).toBeNull();
  });

  it("does not claim no pipelines exist when the scope is why the list is empty", () => {
    renderResults(body({ out_of_scope_pipelines: 5 }));

    expect(
      screen.getByText(/No compiled toast pipelines in the workspaces your platform scope reaches/)
    ).toBeInTheDocument();
  });

  /**
   * "Not scored: 5 compiled pipelines…" and "No compiled toast pipelines in the
   * workspaces your platform scope reaches." are both true and are one idea, so
   * an operator with nothing in scope used to read it twice, stacked. One
   * statement carries both halves.
   */
  it("says the empty scan and the withheld remainder once, not twice", () => {
    renderResults(body({ out_of_scope_pipelines: 5 }));

    const note = screen.getByTestId("admin-airway-out-of-scope-toast");
    expect(note).toHaveTextContent(
      "No compiled toast pipelines in the workspaces your platform scope reaches, and the 5 " +
        "compiled pipelines in the workspaces it does not reach were not scored. Saving the " +
        "global toast policy still applies to them."
    );
    // One paragraph makes the point, not two.
    expect(screen.getAllByText(/No compiled toast pipelines/)).toHaveLength(1);
    expect(screen.queryByText(/^Not scored:/)).toBeNull();
  });

  it("still says none found when nothing was withheld", () => {
    renderResults(body());
    expect(screen.getByText("No compiled toast pipelines found.")).toBeInTheDocument();
  });
});
