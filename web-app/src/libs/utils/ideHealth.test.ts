import { AxiosError, AxiosHeaders } from "axios";
import { describe, expect, it } from "vitest";
import { isIdeUnavailableError } from "./ideHealth";

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
    expect(isIdeUnavailableError(axiosErr(502, { "x-oxy-required-role": "ide" }))).toBe(true);
    // The capability class is irrelevant to detection — both are ide-down.
    expect(
      isIdeUnavailableError(
        axiosErr(502, { "x-oxy-required-role": "ide", "x-oxy-unavailable": "workspace-runtime" })
      )
    ).toBe(true);
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
});
