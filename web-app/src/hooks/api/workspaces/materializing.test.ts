import { AxiosError, AxiosHeaders } from "axios";
import { describe, expect, it } from "vitest";
import { deriveMaterializingState, shouldRetryWorkspaceQuery } from "./materializing";

/** The workspace-materializing 503 the ide returns before its volume is ready. */
function materializing(): AxiosError {
  const err = new AxiosError("not ready");
  err.response = {
    status: 503,
    statusText: "",
    data: undefined,
    headers: new AxiosHeaders({ "x-oxy-unavailable": "workspace-materializing" }),
    config: { headers: new AxiosHeaders() }
  } as AxiosError["response"];
  return err;
}

function serverError(): AxiosError {
  const err = new AxiosError("boom");
  err.response = {
    status: 500,
    statusText: "",
    data: undefined,
    headers: new AxiosHeaders(),
    config: { headers: new AxiosHeaders() }
  } as AxiosError["response"];
  return err;
}

describe("shouldRetryWorkspaceQuery", () => {
  it("gives a materializing workspace a long leash", () => {
    expect(shouldRetryWorkspaceQuery(0, materializing())).toBe(true);
    expect(shouldRetryWorkspaceQuery(23, materializing())).toBe(true);
  });

  it("stops retrying a materializing workspace once the cap is hit", () => {
    // The cap has to bite, or nothing ever transitions to the timed-out state
    // and the shell spins forever.
    expect(shouldRetryWorkspaceQuery(24, materializing())).toBe(false);
  });

  it("keeps React Query's default retry count for every other error", () => {
    // Regression: supplying a custom predicate REPLACES the default, so a naive
    // `isMaterializing && ...` silently dropped real failures to zero retries.
    expect(shouldRetryWorkspaceQuery(0, serverError())).toBe(true);
    expect(shouldRetryWorkspaceQuery(2, serverError())).toBe(true);
    expect(shouldRetryWorkspaceQuery(3, serverError())).toBe(false);
  });
});

describe("deriveMaterializingState", () => {
  it("is 'materializing' while retrying — read off failureReason, not error", () => {
    // React Query keeps the query PENDING and leaves `error` null while it
    // retries; the interim error is on `failureReason`. Reading `error` here
    // made the flag false for the whole retry window.
    const s = deriveMaterializingState({
      isPending: true,
      isError: false,
      error: null,
      failureReason: materializing()
    });
    expect(s.isMaterializing).toBe(true);
    expect(s.materializingTimedOut).toBe(false);
  });

  it("flips to timed-out once retries are exhausted", () => {
    const s = deriveMaterializingState({
      isPending: false,
      isError: true,
      error: materializing(),
      failureReason: materializing()
    });
    expect(s.isMaterializing).toBe(false);
    expect(s.materializingTimedOut).toBe(true);
  });

  it("never reports both at once", () => {
    // The shell picks its branch from these; overlapping would make the
    // spinner and the terminal surface both claim the page.
    for (const q of [
      { isPending: true, isError: false, error: null, failureReason: materializing() },
      { isPending: false, isError: true, error: materializing(), failureReason: materializing() }
    ]) {
      const s = deriveMaterializingState(q);
      expect(s.isMaterializing && s.materializingTimedOut).toBe(false);
    }
  });

  it("claims neither state for an unrelated failure", () => {
    // A real 500 must fall through to the generic error path (toast + redirect),
    // not get swallowed by the calm 'starting up' surface.
    const s = deriveMaterializingState({
      isPending: false,
      isError: true,
      error: serverError(),
      failureReason: serverError()
    });
    expect(s.isMaterializing).toBe(false);
    expect(s.materializingTimedOut).toBe(false);
  });

  it("claims neither state on a clean load", () => {
    const s = deriveMaterializingState({
      isPending: false,
      isError: false,
      error: null,
      failureReason: null
    });
    expect(s.isMaterializing).toBe(false);
    expect(s.materializingTimedOut).toBe(false);
  });
});
