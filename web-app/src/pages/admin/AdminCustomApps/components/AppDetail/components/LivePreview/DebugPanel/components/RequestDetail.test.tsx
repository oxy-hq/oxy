// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OxyRequestEntry } from "../../useOxyRequestLog";
import { RequestDetail } from "./RequestDetail";

vi.mock("../../../CallWaterfall/InvocationCalls", () => ({
  InvocationCalls: ({ appId, invocationId }: { appId: string; invocationId: string }) => (
    <div data-testid='admin-app-invocation-calls'>{`${appId}/${invocationId}`}</div>
  )
}));

afterEach(cleanup);

const entry = (over: Partial<OxyRequestEntry>): OxyRequestEntry => ({
  id: 1,
  method: "POST",
  path: "/customer-apps/acme/pulse-demo/fn/store",
  url: "http://127.0.0.1:5173/customer-apps/acme/pulse-demo/fn/store",
  status: 200,
  ms: 120,
  ok: true,
  at: 0,
  ...over
});

describe("RequestDetail server side", () => {
  it("opens the invocation a function call's answer names", async () => {
    render(
      <RequestDetail
        appId='app-1'
        entry={entry({ resHeaders: { "x-oxy-invocation-id": "iv-9" } })}
        onClose={vi.fn()}
      />
    );
    await userEvent.click(screen.getByTestId("admin-app-debug-server-side-tab"));
    expect(screen.getByTestId("admin-app-invocation-calls")).toHaveTextContent("app-1/iv-9");
    expect(screen.queryByTestId("admin-app-debug-server-side-replay")).not.toBeInTheDocument();
  });

  it("says no invocation ran when the answer names none", async () => {
    render(<RequestDetail appId='app-1' entry={entry({ resHeaders: {} })} onClose={vi.fn()} />);
    await userEvent.click(screen.getByTestId("admin-app-debug-server-side-tab"));
    expect(screen.getByTestId("admin-app-debug-server-side-none")).toHaveTextContent(
      "No invocation ran for this call"
    );
    expect(screen.queryByTestId("admin-app-invocation-calls")).not.toBeInTheDocument();
  });

  it("warns that a keyed call may be answered by the invocation it replays", async () => {
    render(
      <RequestDetail
        appId='app-1'
        entry={entry({
          reqHeaders: { "idempotency-key": "k-1" },
          resHeaders: { "x-oxy-invocation-id": "iv-1" }
        })}
        onClose={vi.fn()}
      />
    );
    await userEvent.click(screen.getByTestId("admin-app-debug-server-side-tab"));
    expect(screen.getByTestId("admin-app-debug-server-side-replay")).toBeInTheDocument();
  });

  it("offers no server side for an API call", () => {
    render(
      <RequestDetail
        appId='app-1'
        entry={entry({ path: "/api/x", url: "http://h/api/customer-apps/x/query" })}
        onClose={vi.fn()}
      />
    );
    expect(screen.queryByTestId("admin-app-debug-server-side-tab")).not.toBeInTheDocument();
  });
});
