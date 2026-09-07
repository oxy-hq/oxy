import { describe, expect, it } from "vitest";
import { percentFromRaw, rawFromPercent } from "./leverPercent";

describe("percentFromRaw", () => {
  it("reads a signed percentage", () => {
    expect(percentFromRaw("+15%")).toBe(15);
    expect(percentFromRaw("-15%")).toBe(-15);
  });

  it("reads an unsigned percentage", () => {
    expect(percentFromRaw("15%")).toBe(15);
  });

  it("tolerates whitespace", () => {
    expect(percentFromRaw("  +15 % ".replace(" %", "%"))).toBe(15);
  });

  // Not errors — legitimate typed values the slider cannot represent, so the
  // caller parks the handle rather than overwriting what was typed.
  it("returns null for an absolute target", () => {
    expect(percentFromRaw("11")).toBe(null);
  });

  it("returns null for a signed delta", () => {
    expect(percentFromRaw("+3")).toBe(null);
    expect(percentFromRaw("-3")).toBe(null);
  });

  it("returns null for empty or nonsense", () => {
    expect(percentFromRaw("")).toBe(null);
    expect(percentFromRaw("abc%")).toBe(null);
  });
});

describe("rawFromPercent", () => {
  it("always signs a positive value so it is not read as an absolute target", () => {
    expect(rawFromPercent(15)).toBe("+15%");
  });

  it("keeps a negative sign", () => {
    expect(rawFromPercent(-15)).toBe("-15%");
  });

  it("renders zero as an explicit no-change", () => {
    expect(rawFromPercent(0)).toBe("0%");
  });

  it("rounds to whole percent", () => {
    expect(rawFromPercent(15.4)).toBe("+15%");
    expect(rawFromPercent(-15.6)).toBe("-16%");
  });

  it("round-trips through percentFromRaw", () => {
    for (const pct of [-100, -37, 0, 42, 100]) {
      expect(percentFromRaw(rawFromPercent(pct))).toBe(pct);
    }
  });
});
