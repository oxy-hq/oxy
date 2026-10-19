/** The two server name rules, as `oxyc`, the Vite plugin and the loader all apply them. */

import { describe, expect, it } from "vitest";
import {
  FUNCTION_NAME_PATTERN,
  isValidFunctionName,
  isValidSlug,
  SLUG_MAX_LEN,
  SLUG_PATTERN
} from "./manifest-rules";

describe("isValidSlug", () => {
  it("accepts lower kebab up to 63 characters", () => {
    for (const ok of ["a", "store-pulse", "a1-b2", "oltp-bookings", "x".repeat(SLUG_MAX_LEN)]) {
      expect(isValidSlug(ok), ok).toBe(true);
    }
  });

  /** An underscore is the one people reach for; the slug becomes a schema name. */
  it("rejects every other shape", () => {
    for (const bad of ["", "-a", "a-", "a--b", "A", "a_b", "a b", "x".repeat(SLUG_MAX_LEN + 1)]) {
      expect(isValidSlug(bad), bad).toBe(false);
    }
  });

  it("is the pattern the schema carries, plus the length", () => {
    expect(SLUG_PATTERN.test("a--b")).toBe(false);
    expect(SLUG_PATTERN.test("x".repeat(100))).toBe(true);
  });
});

describe("isValidFunctionName", () => {
  it("follows the server's function-name grammar", () => {
    for (const ok of ["a", "top-stores", "a1", `a${"b".repeat(63)}`]) {
      expect(isValidFunctionName(ok), ok).toBe(true);
    }
    for (const bad of ["", "1a", "-a", "A", "a_b", "../x", `a${"b".repeat(64)}`]) {
      expect(isValidFunctionName(bad), bad).toBe(false);
    }
    expect(FUNCTION_NAME_PATTERN.source).toBe("^[a-z][a-z0-9-]{0,63}$");
  });
});
