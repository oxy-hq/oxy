import { describe, expect, it } from "vitest";
import { badgeLabel } from "./utils";

describe("badgeLabel", () => {
  it("is nothing at zero", () => {
    expect(badgeLabel(0)).toBeNull();
    expect(badgeLabel(-1)).toBeNull();
  });

  it("is the count up to 99, then 99+", () => {
    expect(badgeLabel(1)).toBe("1");
    expect(badgeLabel(99)).toBe("99");
    expect(badgeLabel(100)).toBe("99+");
    expect(badgeLabel(4321)).toBe("99+");
  });
});
