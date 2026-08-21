import { describe, expect, it } from "vitest";
import { credentialAge, STALE_AFTER_DAYS, ttlLabel } from "./credentialAge";

const NOW = Date.parse("2026-08-21T00:00:00Z");
const daysAgo = (n: number) => new Date(NOW - n * 86_400_000).toISOString();
const at = (msAgo: number) => new Date(NOW - msAgo).toISOString();

describe("credentialAge", () => {
  it("separates a never-rotated new tenant from a never-rotated old one", () => {
    // The whole reason this module exists: both used to read "never".
    const fresh = credentialAge({ sa_rotated_at: null, sa_created_at: daysAgo(3) }, NOW);
    const stale = credentialAge({ sa_rotated_at: null, sa_created_at: daysAgo(400) }, NOW);

    expect(fresh.label).toBe("never · 3d old");
    expect(fresh.overdue).toBe(false);
    expect(stale.label).toBe("never · 400d old");
    expect(stale.overdue).toBe(true);
  });

  it("reads a rotation as its age, and flags one past the threshold", () => {
    expect(credentialAge({ sa_rotated_at: daysAgo(2), sa_created_at: daysAgo(9) }, NOW)).toEqual({
      label: "2d ago",
      overdue: false
    });
    expect(
      credentialAge({ sa_rotated_at: daysAgo(STALE_AFTER_DAYS), sa_created_at: null }, NOW)
    ).toEqual({ label: `${STALE_AFTER_DAYS}d ago`, overdue: true });
  });

  it("does not call a tenant with no account overdue", () => {
    // Rotation age is not the question there, and saying "overdue" would point
    // the operator at the wrong fix — the account needs binding, not rotating.
    expect(credentialAge({ sa_rotated_at: null, sa_created_at: null }, NOW)).toEqual({
      label: "—",
      overdue: false
    });
  });

  it("does not claim `never rotated` when it cannot read the timestamp", () => {
    // The false-but-reassuring answer this module exists to prevent: falling
    // through to the created-at branch would render `never · 400d old` for a
    // credential that WAS rotated, at a time we failed to parse.
    expect(
      credentialAge({ sa_rotated_at: "not-a-date", sa_created_at: daysAgo(400) }, NOW)
    ).toEqual({ label: "unknown", overdue: false });
    expect(credentialAge({ sa_rotated_at: null, sa_created_at: "not-a-date" }, NOW)).toEqual({
      label: "unknown",
      overdue: false
    });
    // And "unknown" is distinct from the glyph for "no account bound at all",
    // which is the state this module separates elsewhere.
    expect(credentialAge({ sa_rotated_at: null, sa_created_at: null }, NOW).label).toBe("—");
  });

  it("keeps sub-day resolution, which is when a rotation is being confirmed", () => {
    // Days alone rendered a credential rotated three hours ago as `0d ago` —
    // the one moment an operator reloads this page specifically to check.
    expect(
      credentialAge({ sa_rotated_at: at(3 * 3_600_000), sa_created_at: null }, NOW).label
    ).toBe("3h ago");
    expect(credentialAge({ sa_rotated_at: at(90_000), sa_created_at: null }, NOW).label).toBe(
      "1m ago"
    );
    expect(credentialAge({ sa_rotated_at: at(5_000), sa_created_at: null }, NOW).label).toBe(
      "just now"
    );
  });
});

describe("ttlLabel", () => {
  it("says a lifetime the way an operator would", () => {
    expect(ttlLabel(3600)).toBe("1h");
    expect(ttlLabel(7200)).toBe("2h");
    expect(ttlLabel(900)).toBe("15m");
    expect(ttlLabel(45)).toBe("45s");
  });

  it("reads an absent lifetime as absent", () => {
    expect(ttlLabel(null)).toBe("—");
    // `=== null` rendered this as "undefineds".
    expect(ttlLabel(undefined)).toBe("—");
  });

  it("does not call zero an hour", () => {
    // `0 % 3600 === 0`, so the hour branch claimed it first.
    expect(ttlLabel(0)).toBe("0s");
  });
});
