// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GlobalNotificationSettings, QuietHours } from "@/types/notifications";
import { QuietHoursRow } from "./QuietHoursRow";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const QUIET: QuietHours = {
  start: "22:00",
  end: "07:00",
  days: [1, 2, 3, 4, 5],
  allow: "critical"
};
const global = (over: Partial<GlobalNotificationSettings> = {}): GlobalNotificationSettings => ({
  paused_until: null,
  quiet_hours: QUIET,
  timezone: "Europe/Berlin",
  ...over
});

function renderRow(settings: GlobalNotificationSettings = global()) {
  const onChange = vi.fn();
  render(<QuietHoursRow global={settings} onChange={onChange} />);
  return onChange;
}

const setTime = (testId: string, value: string) => {
  const input = screen.getByTestId(testId);
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
};

describe("QuietHoursRow", () => {
  it("is off, with nothing to edit, until switched on", () => {
    renderRow(global({ quiet_hours: null, timezone: null }));
    expect(screen.getByTestId("notifications-quiet-toggle")).not.toBeChecked();
    expect(screen.queryByTestId("notifications-quiet-start")).toBeNull();
  });

  it("switches on overnight, every day, critical only, in this browser's time zone", async () => {
    vi.spyOn(Intl.DateTimeFormat.prototype, "resolvedOptions").mockReturnValue({
      timeZone: "America/Chicago"
    } as Intl.ResolvedDateTimeFormatOptions);
    const onChange = renderRow(global({ quiet_hours: null, timezone: null }));
    await userEvent.click(screen.getByTestId("notifications-quiet-toggle"));
    expect(onChange).toHaveBeenCalledWith({
      quiet_hours: { start: "22:00", end: "07:00", days: [0, 1, 2, 3, 4, 5, 6], allow: "critical" },
      timezone: "America/Chicago"
    });
  });

  it("switches off", async () => {
    const onChange = renderRow();
    await userEvent.click(screen.getByTestId("notifications-quiet-toggle"));
    expect(onChange).toHaveBeenCalledWith({ quiet_hours: null });
  });

  it("shows the stored time zone", () => {
    renderRow();
    expect(screen.getByTestId("notifications-quiet-timezone")).toHaveTextContent("Europe/Berlin");
  });

  it("saves a new start when the field is left", () => {
    const onChange = renderRow();
    setTime("notifications-quiet-start", "23:30");
    expect(onChange).toHaveBeenCalledWith({ quiet_hours: { ...QUIET, start: "23:30" } });
  });

  it("refuses a window that starts when it ends, and saves nothing", () => {
    const onChange = renderRow();
    setTime("notifications-quiet-end", "22:00");
    expect(screen.getByTestId("notifications-quiet-error")).toHaveTextContent(
      "Start and end must differ"
    );
    expect(onChange).not.toHaveBeenCalled();
  });

  it("refuses a cleared time", () => {
    const onChange = renderRow();
    setTime("notifications-quiet-start", "");
    expect(screen.getByTestId("notifications-quiet-error")).toHaveTextContent(
      "Use a time like 22:00"
    );
    expect(onChange).not.toHaveBeenCalled();
  });

  it("adds and removes days, in order", async () => {
    const onChange = renderRow();
    await userEvent.click(screen.getByTestId("notifications-quiet-day-0"));
    expect(onChange).toHaveBeenLastCalledWith({
      quiet_hours: { ...QUIET, days: [0, 1, 2, 3, 4, 5] }
    });
    await userEvent.click(screen.getByTestId("notifications-quiet-day-3"));
    expect(onChange).toHaveBeenLastCalledWith({ quiet_hours: { ...QUIET, days: [1, 2, 4, 5] } });
  });

  it("keeps at least one day", async () => {
    const onChange = renderRow(global({ quiet_hours: { ...QUIET, days: [3] } }));
    await userEvent.click(screen.getByTestId("notifications-quiet-day-3"));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("lets time-sensitive through", async () => {
    const onChange = renderRow();
    await userEvent.click(screen.getByTestId("notifications-quiet-allow"));
    expect(onChange).toHaveBeenCalledWith({ quiet_hours: { ...QUIET, allow: "time_sensitive" } });
  });
});
