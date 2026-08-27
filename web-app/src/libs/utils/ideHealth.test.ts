import { AxiosError, AxiosHeaders } from "axios";
import { describe, expect, it } from "vitest";
import { isIdeUnavailableError, isWorkspaceMaterializingError } from "./ideHealth";

/** Build an AxiosError with a given status + response headers. */
function axiosErr(status: number, headers: Record<string, string>): AxiosError {
  const err = new AxiosError("boom");
  err.response = {
    status,
    statusText: "",
    data: undefined,
    headers: new AxiosHeaders(headers),
    config: { headers: new AxiosHeaders() }
  } as AxiosError["response"];
  return err;
}

describe("isIdeUnavailableError", () => {
  it("is true for the ide-down 502 (required-role: ide)", () => {
    // This header IS the whole contract. The 502 used to also carry
    // `x-oxy-unavailable: workspace-{runtime,editing}`; nothing branched on it,
    // so the backend stopped sending it — detection was never affected.
    expect(isIdeUnavailableError(axiosErr(502, { "x-oxy-required-role": "ide" }))).toBe(true);
  });

  it("is false for a generic 502 without the ide marker", () => {
    expect(isIdeUnavailableError(axiosErr(502, {}))).toBe(false);
  });

  it("is false for other statuses even with the ide marker", () => {
    expect(isIdeUnavailableError(axiosErr(500, { "x-oxy-required-role": "ide" }))).toBe(false);
    expect(isIdeUnavailableError(axiosErr(421, { "x-oxy-required-role": "ide" }))).toBe(false);
  });

  it("is false for non-axios errors and nullish values", () => {
    expect(isIdeUnavailableError(new Error("plain"))).toBe(false);
    expect(isIdeUnavailableError(undefined)).toBe(false);
    expect(isIdeUnavailableError(null)).toBe(false);
  });

  it("is false for the workspace-materializing 503", () => {
    // The ide is REACHABLE here — only this workspace isn't ready. Raising the
    // global ide-down banner would be wrong, and a concurrent healthy ide
    // response would flap it straight back off.
    expect(
      isIdeUnavailableError(axiosErr(503, { "x-oxy-unavailable": "workspace-materializing" }))
    ).toBe(false);
  });
});

describe("isWorkspaceMaterializingError", () => {
  it("is true for the 503 carrying the materializing class", () => {
    expect(
      isWorkspaceMaterializingError(
        axiosErr(503, { "x-oxy-unavailable": "workspace-materializing" })
      )
    ).toBe(true);
  });

  it("is false for a generic 503 without the class", () => {
    // A plain 503 is a real outage, not a workspace still coming up — it must
    // not be silently retried behind a spinner.
    expect(isWorkspaceMaterializingError(axiosErr(503, {}))).toBe(false);
  });

  it("is false for any other value of the class", () => {
    // `workspace-materializing` is the only value the backend emits today, so
    // this guards the direction that matters: a future class added to this
    // header must not be swallowed as "still coming up".
    expect(
      isWorkspaceMaterializingError(axiosErr(503, { "x-oxy-unavailable": "workspace-elsewhere" }))
    ).toBe(false);
  });

  it("is false for other statuses even with the class", () => {
    // Notably the ide-down 502: the two signals must stay disjoint, so neither
    // detector can claim the other's response.
    expect(
      isWorkspaceMaterializingError(
        axiosErr(502, { "x-oxy-unavailable": "workspace-materializing" })
      )
    ).toBe(false);
    expect(
      isWorkspaceMaterializingError(
        axiosErr(200, { "x-oxy-unavailable": "workspace-materializing" })
      )
    ).toBe(false);
  });

  it("is false for non-axios errors and nullish values", () => {
    expect(isWorkspaceMaterializingError(new Error("plain"))).toBe(false);
    expect(isWorkspaceMaterializingError(undefined)).toBe(false);
    expect(isWorkspaceMaterializingError(null)).toBe(false);
  });
});
