import { describe, expect, it } from "vitest";
import { compactInt, usd } from "./format";

describe("usd", () => {
  it("returns an em dash for nullish (loading/absent), not $0", () => {
    expect(usd(null)).toBe("—");
    expect(usd(undefined)).toBe("—");
  });

  it("distinguishes a real zero from absent", () => {
    expect(usd(0)).toBe("$0");
  });

  it("keeps cents under a dollar", () => {
    expect(usd(0.42)).toBe("$0.42");
  });

  it("rounds whole dollars under 1k", () => {
    expect(usd(412.5)).toBe("$413");
    expect(usd(999)).toBe("$999");
  });

  it("compacts thousands and millions", () => {
    expect(usd(1234)).toBe("$1.2k");
    expect(usd(3_400_000)).toBe("$3.4M");
  });
});

describe("compactInt", () => {
  it("returns an em dash for nullish", () => {
    expect(compactInt(null)).toBe("—");
    expect(compactInt(undefined)).toBe("—");
  });

  it("uses grouped digits below 10k", () => {
    expect(compactInt(1204)).toBe("1,204");
    expect(compactInt(0)).toBe("0");
  });

  it("compacts large counts", () => {
    expect(compactInt(12_000)).toBe("12.0k");
    expect(compactInt(3_400_000)).toBe("3.4M");
  });
});
