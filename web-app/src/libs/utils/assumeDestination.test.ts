// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  clearAssumeDestination,
  peekAssumeReturnTo,
  rememberAssumeDestination,
  takeAssumeLanding
} from "./assumeDestination";

const ORG = "11111111-1111-1111-1111-111111111111";
const OTHER_ORG = "22222222-2222-2222-2222-222222222222";
const LANDING = "/customer-apps/acme/sales/";
const RETURN_TO = "/admin/apps/acme/sales";

afterEach(() => {
  clearAssumeDestination();
  vi.useRealTimers();
});

describe("assumeDestination", () => {
  it("carries the round trip across the two page loads", () => {
    rememberAssumeDestination({ orgId: ORG, landing: LANDING, returnTo: RETURN_TO });

    expect(takeAssumeLanding(ORG)).toBe(LANDING);
    // The trip home outlives the arrival — the session is still live.
    expect(peekAssumeReturnTo(ORG)).toBe(RETURN_TO);
  });

  it("consumes the landing so a later session doesn't inherit it", () => {
    rememberAssumeDestination({ orgId: ORG, landing: LANDING, returnTo: RETURN_TO });

    expect(takeAssumeLanding(ORG)).toBe(LANDING);
    expect(takeAssumeLanding(ORG)).toBeNull();
  });

  it("only answers for the org the trip was recorded for", () => {
    rememberAssumeDestination({ orgId: ORG, landing: LANDING, returnTo: RETURN_TO });

    expect(takeAssumeLanding(OTHER_ORG)).toBeNull();
    expect(peekAssumeReturnTo(OTHER_ORG)).toBeNull();
    expect(peekAssumeReturnTo(undefined)).toBeNull();
  });

  // Both values are fed to `window.location.assign`, and localStorage is
  // user-writable: an off-site destination must never be storable.
  it.each([
    ["protocol-relative", "//evil.example.com/"],
    ["absolute", "https://evil.example.com/"],
    ["backslash", "/\\evil.example.com/"],
    // The URL spec strips tab/LF/CR before parsing, so these collapse to a
    // protocol-relative `//evil…` when handed to location.assign.
    ["tab-smuggled", "/\t/evil.example.com/"],
    ["newline-smuggled", "/\n/evil.example.com/"],
    ["cr-smuggled", "/\r/evil.example.com/"],
    ["relative", "customer-apps/acme/sales/"]
  ])("refuses a %s destination", (_label, hostile) => {
    rememberAssumeDestination({ orgId: ORG, landing: hostile, returnTo: RETURN_TO });
    expect(takeAssumeLanding(ORG)).toBeNull();

    rememberAssumeDestination({ orgId: ORG, landing: LANDING, returnTo: hostile });
    expect(peekAssumeReturnTo(ORG)).toBeNull();
  });

  // A record outliving the server's non-renewable 60-minute ceiling describes a
  // session that no longer exists.
  it("expires with the session", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T10:00:00Z"));
    rememberAssumeDestination({ orgId: ORG, landing: LANDING, returnTo: RETURN_TO });

    vi.setSystemTime(new Date("2026-07-24T10:59:00Z"));
    expect(peekAssumeReturnTo(ORG)).toBe(RETURN_TO);

    vi.setSystemTime(new Date("2026-07-24T11:00:01Z"));
    expect(peekAssumeReturnTo(ORG)).toBeNull();
    expect(takeAssumeLanding(ORG)).toBeNull();
  });

  it("forgets the trip on clear", () => {
    rememberAssumeDestination({ orgId: ORG, landing: LANDING, returnTo: RETURN_TO });
    clearAssumeDestination();

    expect(peekAssumeReturnTo(ORG)).toBeNull();
    expect(takeAssumeLanding(ORG)).toBeNull();
  });
});
