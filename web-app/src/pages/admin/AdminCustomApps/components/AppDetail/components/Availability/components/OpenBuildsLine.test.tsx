// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { OpenBuilds } from "@/types/apps";
import { OpenBuildsLine } from "./OpenBuildsLine";

const open = (patch: Partial<OpenBuilds>): OpenBuilds => ({
  status: "none_seen",
  live_sessions: 0,
  older_sessions: 0,
  older_builds: 0,
  window_minutes: 1440,
  ...patch
});

/**
 * The operator line for "have open tabs picked up the last publish". The rule
 * it exists to keep: an unmeasured answer never reads as everyone being live.
 */
describe("OpenBuildsLine", () => {
  afterEach(cleanup);

  it("says not measured, and never claims anyone is on the live build", () => {
    render(<OpenBuildsLine open={open({ status: "not_measured", live_sessions: 3 })} />);
    const line = screen.getByTestId("admin-app-open-builds");
    expect(line.textContent).toContain("not measured");
    expect(line.textContent).not.toContain("live build");
  });

  it("counts sessions left on older builds", () => {
    render(
      <OpenBuildsLine
        open={open({ status: "some_older", live_sessions: 12, older_sessions: 3, older_builds: 2 })}
      />
    );
    expect(screen.getByTestId("admin-app-open-builds").textContent).toBe(
      "Open tabs: 3 sessions on 2 older builds, 12 on the live build (last 24h)."
    );
  });

  it("tells all-live apart from nothing seen", () => {
    const { rerender } = render(
      <OpenBuildsLine open={open({ status: "all_live", live_sessions: 1 })} />
    );
    expect(screen.getByTestId("admin-app-open-builds").textContent).toContain(
      "all 1 session seen in the last 24h are on the live build (a tab offline or signed out that long is not seen)"
    );
    rerender(<OpenBuildsLine open={open({ status: "none_seen" })} />);
    expect(screen.getByTestId("admin-app-open-builds").textContent).toContain(
      "no session reported its build"
    );
  });

  it("renders nothing for a server that predates the field", () => {
    const { container } = render(<OpenBuildsLine open={undefined} />);
    expect(container.textContent).toBe("");
  });
});
