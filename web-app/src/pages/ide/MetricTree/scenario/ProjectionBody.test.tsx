// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { MeasureProjection } from "@/types/metricTree";
import { ProjectionBody } from "./ProjectionBody";
import type { ScenarioCurve } from "./projectionCurves";

afterEach(cleanup);

function projection(): MeasureProjection {
  return {
    measure: "labor.total_regular_hours",
    history: [{ date: "2026-08-01", value: 100 }],
    forecast: [{ date: "2026-08-02", point: 101, lower: 95, upper: 107 }],
    seasonality: [7]
  };
}

function curve(): ScenarioCurve {
  return {
    kind: "curve",
    points: [{ date: "2026-08-02", value: 103 }],
    landsAt: "2026-08-02",
    confidence: "estimated"
  };
}

describe("ProjectionBody", () => {
  /** The panel used to render as an empty hole for the whole of the first
   *  request — several seconds of warehouse query plus fit, with nothing on
   *  screen saying so. */
  it("says a projection is running when there is no chart to say it on", () => {
    render(<ProjectionBody note={null} projection={undefined} curve={null} isFetching />);

    expect(screen.getByTestId("projection-pending")).toBeTruthy();
  });

  /** Every control on the panel changes the query key, so each one drops back
   *  to having no curves at all rather than refetching under the old ones. */
  it("says so again when a control change throws the curves away", () => {
    const { rerender } = render(
      <ProjectionBody note={null} projection={projection()} curve={curve()} isFetching={false} />
    );
    expect(screen.queryByTestId("projection-pending")).toBeNull();

    rerender(<ProjectionBody note={null} projection={undefined} curve={null} isFetching />);

    expect(screen.getByTestId("projection-pending")).toBeTruthy();
  });

  /** A refetch that keeps its curves is the chart's own overlay to draw.
   *  Stacking a placeholder under it would push the chart down mid-refresh. */
  it("leaves a refetch over existing curves to the chart", () => {
    render(<ProjectionBody note={null} projection={projection()} curve={curve()} isFetching />);

    expect(screen.queryByTestId("projection-pending")).toBeNull();
    expect(screen.getByTestId("metric-tree-projection-chart")).toBeTruthy();
  });

  /** Idle with nothing to draw is not the same as working on it: that state
   *  belongs to the note explaining what the panel is still waiting for. */
  it("shows nothing spinning when no request is in flight", () => {
    render(
      <ProjectionBody
        note='pin a lever and pick a time dimension to project forward'
        projection={undefined}
        curve={null}
        isFetching={false}
      />
    );

    expect(screen.queryByTestId("projection-pending")).toBeNull();
    expect(screen.getByTestId("projection-note")).toBeTruthy();
  });

  /** A note explaining what the panel is waiting on has to survive the state
   *  where the curves themselves are not there yet. */
  it("keeps the note visible while pending", () => {
    render(
      <ProjectionBody
        note='no history for this measure'
        projection={undefined}
        curve={null}
        isFetching
      />
    );

    expect(screen.getByTestId("projection-pending")).toBeTruthy();
    expect(screen.getByTestId("projection-note")).toBeTruthy();
  });
});
