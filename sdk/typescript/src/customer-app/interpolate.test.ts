// Unit tests for the SQL param interpolation logic used by useQuery.

import { describe, expect, it } from "vitest";
import { interpolateSqlParams } from "./interpolate";

describe("interpolateSqlParams", () => {
  // ── | sqlquote filter ──────────────────────────────────────────────────────

  it("sqlquote: wraps string values in single quotes", () => {
    const sql = "SELECT * FROM t WHERE store = {{ params.store | sqlquote }}";
    expect(interpolateSqlParams(sql, { store: "Acme" })).toBe(
      "SELECT * FROM t WHERE store = 'Acme'"
    );
  });

  it("sqlquote: escapes single quotes inside string values", () => {
    const sql = "SELECT * FROM t WHERE name = {{ params.name | sqlquote }}";
    expect(interpolateSqlParams(sql, { name: "O'Brien" })).toBe(
      "SELECT * FROM t WHERE name = 'O''Brien'"
    );
  });

  it("sqlquote: leaves numbers raw (no quotes)", () => {
    const sql = "SELECT * FROM t WHERE id = {{ params.id | sqlquote }}";
    expect(interpolateSqlParams(sql, { id: 42 })).toBe("SELECT * FROM t WHERE id = 42");
  });

  it("sqlquote: leaves booleans raw (no quotes)", () => {
    const sql = "SELECT * FROM t WHERE active = {{ params.active | sqlquote }}";
    expect(interpolateSqlParams(sql, { active: true })).toBe("SELECT * FROM t WHERE active = true");
  });

  it("sqlquote: replaces null with NULL", () => {
    const sql = "SELECT * FROM t WHERE val = {{ params.val | sqlquote }}";
    expect(interpolateSqlParams(sql, { val: null })).toBe("SELECT * FROM t WHERE val = NULL");
  });

  it("sqlquote: replaces undefined with NULL", () => {
    const sql = "SELECT * FROM t WHERE val = {{ params.val | sqlquote }}";
    expect(interpolateSqlParams(sql, { val: undefined })).toBe("SELECT * FROM t WHERE val = NULL");
  });

  it("sqlquote: replaces missing keys with NULL", () => {
    const sql = "SELECT * FROM t WHERE x = {{ params.missing | sqlquote }}";
    expect(interpolateSqlParams(sql, {})).toBe("SELECT * FROM t WHERE x = NULL");
  });

  // ── no filter (raw pass-through) ──────────────────────────────────────────

  it("no filter: passes string through raw (no quotes)", () => {
    const sql = "SELECT * FROM t WHERE store = {{ params.store }}";
    expect(interpolateSqlParams(sql, { store: "LA" })).toBe("SELECT * FROM t WHERE store = LA");
  });

  it("no filter: passes number through raw", () => {
    const sql = "SELECT * FROM t WHERE id = {{ params.id }}";
    expect(interpolateSqlParams(sql, { id: 42 })).toBe("SELECT * FROM t WHERE id = 42");
  });

  it("no filter: replaces null with NULL", () => {
    const sql = "SELECT * FROM t WHERE val = {{ params.val }}";
    expect(interpolateSqlParams(sql, { val: null })).toBe("SELECT * FROM t WHERE val = NULL");
  });

  it("no filter: replaces missing keys with NULL", () => {
    const sql = "SELECT * FROM t WHERE x = {{ params.missing }}";
    expect(interpolateSqlParams(sql, {})).toBe("SELECT * FROM t WHERE x = NULL");
  });

  // ── mixed placeholders ─────────────────────────────────────────────────────

  it("handles multiple placeholders mixing sqlquote and raw in the same query", () => {
    const sql =
      "SELECT * FROM t WHERE store = {{ params.store | sqlquote }} AND year = {{ params.year }}";
    expect(interpolateSqlParams(sql, { store: "NY", year: 2024 })).toBe(
      "SELECT * FROM t WHERE store = 'NY' AND year = 2024"
    );
  });

  it("leaves SQL unchanged when there are no placeholders", () => {
    const sql = "SELECT 1";
    expect(interpolateSqlParams(sql, { unused: "val" })).toBe("SELECT 1");
  });
});
