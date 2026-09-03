/**
 * The pass-through half of the bare-customer form.
 *
 * Deliberately a unit test and not a binary one: a token that is NOT a near
 * miss reaches `launch`, which resolves a customer over the network and can
 * clone a repo. Asserting the decision here keeps it fast and side-effect
 * free; `main.test.ts` covers the branch that short-circuits before any work.
 */

import { describe, expect, it } from "vitest";
import { didYouMeanCommand, editDistance } from "./suggest.js";

const COMMANDS = ["api", "routes", "schema", "openapi", "doctor", "update", "adopt", "launch"];

describe("didYouMeanCommand", () => {
  it("catches a transposition, a drop and a doubling", () => {
    expect(didYouMeanCommand("rotues", COMMANDS)).toBe("routes");
    expect(didYouMeanCommand("doctro", COMMANDS)).toBe("doctor");
    expect(didYouMeanCommand("openapii", COMMANDS)).toBe("openapi");
  });

  /**
   * A real customer name must NOT be captured. `pokehouse-oxy` is nothing like
   * a command, and treating it as a typo would break the flagship interaction.
   */
  it("leaves a name that resembles no command alone", () => {
    expect(didYouMeanCommand("pokehouse-oxy", COMMANDS)).toBeUndefined();
    expect(didYouMeanCommand("bmg-industries-oxy", COMMANDS)).toBeUndefined();
  });

  /**
   * Short tokens get a tighter budget: two edits on a four-letter word is most
   * of the word, so `acme` must not be read as a mistyped `api`.
   */
  it("is stricter about short tokens", () => {
    expect(didYouMeanCommand("acme", COMMANDS)).toBeUndefined();
    // …but one edit on a short token is still a typo worth catching.
    expect(didYouMeanCommand("api2", COMMANDS)).toBe("api");
  });

  /**
   * A TIE MUST BE STABLE. Two commands at equal distance are resolved by list
   * order — arbitrary, but the same on every run and every machine. Resolving
   * by "whichever came last" would make the suggestion depend on how
   * `buildProgram` happens to be ordered, which changes whenever a command is
   * added.
   */
  it("breaks a tie by list order, the same way every time", () => {
    // "aaa" is one edit from both "aab" and "aac".
    expect(didYouMeanCommand("aaa", ["aab", "aac"])).toBe("aab");
    expect(didYouMeanCommand("aaa", ["aac", "aab"])).toBe("aac");
    // …and it is deterministic across repeated calls with the same input.
    const twice = [
      didYouMeanCommand("aaa", ["aab", "aac"]),
      didYouMeanCommand("aaa", ["aab", "aac"])
    ];
    expect(twice[0]).toBe(twice[1]);
  });

  it("returns undefined against an empty command list", () => {
    expect(didYouMeanCommand("anything", [])).toBeUndefined();
  });
});

describe("editDistance", () => {
  it("is zero for identical strings and symmetric", () => {
    expect(editDistance("routes", "routes")).toBe(0);
    expect(editDistance("abc", "abd")).toBe(editDistance("abd", "abc"));
  });

  it("counts an insert, a delete and a substitution as one each", () => {
    expect(editDistance("route", "routes")).toBe(1);
    expect(editDistance("routes", "route")).toBe(1);
    expect(editDistance("routes", "rootes")).toBe(1);
  });

  it("handles an empty string as the length of the other", () => {
    expect(editDistance("", "api")).toBe(3);
    expect(editDistance("api", "")).toBe(3);
  });
});
