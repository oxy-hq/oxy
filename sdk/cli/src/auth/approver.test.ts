/** `--approver`: what is accepted before any request is sent. */

import { describe, expect, it } from "vitest";
import { CliError, ExitCode } from "../util/errors.js";
import { approverFrom, plausibleEmail } from "./approver.js";

const refusal = (run: () => unknown): CliError => {
  try {
    run();
  } catch (cause) {
    expect(cause).toBeInstanceOf(CliError);
    return cause as CliError;
  }
  throw new Error("expected a usage error");
};

describe("approverFrom", () => {
  it("is the trimmed email with --no-browser, and nothing when none was given", () => {
    expect(approverFrom({ approver: " ada@example.com " }, true)).toBe("ada@example.com");
    expect(approverFrom({}, true)).toBeUndefined();
    expect(approverFrom({}, false)).toBeUndefined();
    // An empty OXY_APPROVER is no approver, with or without --no-browser.
    expect(approverFrom({ approver: "" }, false)).toBeUndefined();
    expect(approverFrom({ approver: "  " }, true)).toBeUndefined();
    // An exported OXY_APPROVER is for the agents; a person's own browser round ignores it.
    expect(approverFrom({ approver: "ada@example.com" }, false, "env")).toBeUndefined();
    expect(approverFrom({ approver: "ada@example.com" }, true, "env")).toBe("ada@example.com");
  });

  it("is a usage error without --no-browser", () => {
    const cause = refusal(() => approverFrom({ approver: "ada@example.com" }, false));
    expect(cause.code).toBe(ExitCode.USAGE);
    expect(cause.message).toBe("--approver is only valid with --no-browser");
  });

  it("is a usage error for something that cannot be an email", () => {
    for (const bad of ["ada", "ada@", "@example.com", "a@b@c", "a da@example.com"]) {
      const cause = refusal(() => approverFrom({ approver: bad }, true));
      expect(cause.code, bad).toBe(ExitCode.USAGE);
      expect(cause.message).toBe("--approver must be an email address");
    }
  });
});

describe("plausibleEmail", () => {
  it("wants one @ with something either side, no whitespace or control, at most 254 characters", () => {
    expect(plausibleEmail("a@b")).toBe(true);
    expect(plausibleEmail(`${"a".repeat(245)}@oxy.tech`)).toBe(true);
    expect(plausibleEmail(`${"a".repeat(246)}@oxy.tech`)).toBe(false);
    expect(plausibleEmail("a@b\n")).toBe(false);
    expect(plausibleEmail("a\u0000@b")).toBe(false);
  });
});
