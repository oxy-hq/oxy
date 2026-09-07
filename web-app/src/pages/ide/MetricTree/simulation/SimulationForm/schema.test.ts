import { describe, expect, it } from "vitest";
import { BARE_COLUMN_NAME_PATTERN, columnNameError } from "./schema";

// The rule under test is a mirror of `is_bare_identifier` +
// `SimulationSpec::validate` in `crates/simulation/src/spec.rs`. Every
// rejected character here is one that corrupts a generated artifact three
// layers down (CSV header, view YAML), so a case dropped from this list is a
// server-side error the author sees instead of an inline one.
describe("BARE_COLUMN_NAME_PATTERN", () => {
  it("accepts a bare identifier with digits and underscores", () => {
    expect(BARE_COLUMN_NAME_PATTERN.test("net_sales_7d")).toBe(true);
    expect(BARE_COLUMN_NAME_PATTERN.test("_leading_underscore")).toBe(true);
    expect(BARE_COLUMN_NAME_PATTERN.test("A")).toBe(true);
  });

  it.each([
    ["a dot (reads as a view.member path)", "sales.net"],
    ["a comma (splits the generated CSV header)", "net,sales"],
    ["a colon (breaks the generated YAML)", "net:sales"],
    ["a space (not resolvable bare in SQL)", "net sales"],
    ["a leading digit", "7d_sales"],
    ["a hyphen", "net-sales"],
    ["a quote", 'net"sales'],
    ["a newline", "net\nsales"],
    ["a leading #", "#net_sales"],
    ["empty", ""]
  ])("rejects %s", (_why, value) => {
    expect(BARE_COLUMN_NAME_PATTERN.test(value)).toBe(false);
  });
});

describe("columnNameError", () => {
  it("passes a valid, non-colliding name", () => {
    expect(columnNameError("net_sales_7d", "marketing_spend")).toBeNull();
  });

  it.each([
    ["a comma", "net,sales"],
    ["a colon", "net:sales"],
    ["a space", "net sales"],
    ["a leading digit", "7d_sales"],
    ["a hyphen", "net-sales"],
    ["a dot", "sales.net"],
    ["empty", ""]
  ])("reports the identifier-class rule for %s", (_why, value) => {
    expect(columnNameError(value, "marketing_spend")).toBe(
      "must be a bare column name: a letter or underscore, then letters, digits or underscores"
    );
  });

  it.each(["entity_id", "date", "prime_cost"])(
    "reports %s as already declared by every generated world",
    (value) => {
      expect(columnNameError(value, "marketing_spend")).toBe(
        `'${value}' is already declared by every generated world (entity_id, date, prime_cost)`
      );
    }
  );

  it("reports a driver/target collision", () => {
    expect(columnNameError("net_sales", "net_sales")).toBe(
      "driver and target must be different columns"
    );
  });
});
