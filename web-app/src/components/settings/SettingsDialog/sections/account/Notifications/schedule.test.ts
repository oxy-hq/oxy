import { afterEach, describe, expect, it, vi } from "vitest";
import type { GlobalNotificationSettings } from "@/types/notifications";
import {
  DEFAULT_QUIET_HOURS,
  formatUntil,
  PAUSE_PRESETS,
  pausedUntil,
  quietHoursChange,
  quietHoursError
} from "./schedule";

// Local time, so "tomorrow 8:00" is the browser's 8:00 whatever zone the test runs in.
const NOW = new Date(2026, 9, 17, 14, 30); // Sat 17 Oct 2026, 2:30 PM
const preset = (id: string) => {
  const found = PAUSE_PRESETS.find((p) => p.id === id);
  if (!found) throw new Error(`no preset ${id}`);
  return found;
};
const global = (over: Partial<GlobalNotificationSettings> = {}): GlobalNotificationSettings => ({
  paused_until: null,
  quiet_hours: null,
  timezone: null,
  ...over
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("pause presets", () => {
  it("offers 1 hour, until tomorrow 8:00, and 1 week, in that order", () => {
    expect(PAUSE_PRESETS.map((p) => p.label)).toEqual(["1 hour", "Until tomorrow 8:00", "1 week"]);
  });

  it("pauses for an hour from now", () => {
    expect(preset("hour").until(NOW)).toEqual(new Date(2026, 9, 17, 15, 30));
  });

  it("pauses until 8:00 the next morning, local time", () => {
    expect(preset("tomorrow").until(NOW)).toEqual(new Date(2026, 9, 18, 8, 0));
    // Just after midnight, "tomorrow" is still the next calendar day.
    expect(preset("tomorrow").until(new Date(2026, 9, 18, 0, 5))).toEqual(
      new Date(2026, 9, 19, 8, 0)
    );
  });

  it("pauses for a week", () => {
    expect(preset("week").until(NOW)).toEqual(new Date(2026, 9, 24, 14, 30));
  });
});

describe("pausedUntil", () => {
  it("is the end of a pause still running", () => {
    const until = new Date(2026, 9, 17, 18, 0);
    expect(pausedUntil(global({ paused_until: until.toISOString() }), NOW)).toEqual(until);
  });

  it("is null when nothing is paused, or the pause is over", () => {
    expect(pausedUntil(global(), NOW)).toBeNull();
    const over = new Date(2026, 9, 17, 14, 0).toISOString();
    expect(pausedUntil(global({ paused_until: over }), NOW)).toBeNull();
  });
});

describe("formatUntil", () => {
  it("says only the time today, 'tomorrow' tomorrow, the weekday this week, else the date", () => {
    expect(formatUntil(new Date(2026, 9, 17, 15, 30), NOW)).toBe("3:30 PM");
    expect(formatUntil(new Date(2026, 9, 18, 8, 0), NOW)).toBe("tomorrow 8:00 AM");
    expect(formatUntil(new Date(2026, 9, 21, 9, 0), NOW)).toBe("Wed 9:00 AM");
    expect(formatUntil(new Date(2026, 9, 24, 14, 30), NOW)).toBe("Oct 24, 2:30 PM");
  });
});

describe("quietHoursError", () => {
  it("accepts an overnight window", () => {
    expect(quietHoursError(DEFAULT_QUIET_HOURS)).toBeNull();
  });

  it("refuses a time that is not HH:MM", () => {
    expect(quietHoursError({ ...DEFAULT_QUIET_HOURS, start: "" })).toBe("Use a time like 22:00");
    expect(quietHoursError({ ...DEFAULT_QUIET_HOURS, end: "24:00" })).toBe("Use a time like 22:00");
    expect(quietHoursError({ ...DEFAULT_QUIET_HOURS, start: "7:00" })).toBe(
      "Use a time like 22:00"
    );
  });

  it("refuses an empty window and no days", () => {
    expect(quietHoursError({ ...DEFAULT_QUIET_HOURS, end: "22:00" })).toBe(
      "Start and end must differ"
    );
    expect(quietHoursError({ ...DEFAULT_QUIET_HOURS, days: [] })).toBe("Pick at least one day");
  });
});

describe("quietHoursChange", () => {
  const zone = (timeZone: string) =>
    vi
      .spyOn(Intl.DateTimeFormat.prototype, "resolvedOptions")
      .mockReturnValue({ timeZone } as Intl.ResolvedDateTimeFormatOptions);

  it("stores this browser's time zone the first time quiet hours are saved", () => {
    zone("Australia/Sydney");
    expect(quietHoursChange(DEFAULT_QUIET_HOURS, global())).toEqual({
      quiet_hours: DEFAULT_QUIET_HOURS,
      timezone: "Australia/Sydney"
    });
  });

  it("keeps a time zone already stored", () => {
    zone("Australia/Sydney");
    expect(quietHoursChange(DEFAULT_QUIET_HOURS, global({ timezone: "Europe/Berlin" }))).toEqual({
      quiet_hours: DEFAULT_QUIET_HOURS
    });
  });

  it("turns quiet hours off without touching the time zone", () => {
    expect(quietHoursChange(null, global())).toEqual({ quiet_hours: null });
  });
});
