// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { GlobalNotificationSettings } from "@/types/notifications";
import { PauseRow } from "./PauseRow";

beforeAll(() => {
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});

// Sat 17 Oct 2026, 2:30 PM local. Only Date is faked: the menus' timers stay real.
const NOW = new Date(2026, 9, 17, 14, 30);
beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const global = (paused_until: string | null = null): GlobalNotificationSettings => ({
  paused_until,
  quiet_hours: null,
  timezone: null
});

async function pick(preset: string) {
  const onChange = vi.fn();
  render(<PauseRow global={global()} onChange={onChange} />);
  await userEvent.click(screen.getByTestId("notifications-pause-open"));
  await userEvent.click(await screen.findByTestId(`notifications-pause-${preset}`));
  return new Date(onChange.mock.calls[0][0]);
}

describe("PauseRow", () => {
  it("offers to pause when nothing is paused", () => {
    render(<PauseRow global={global()} onChange={vi.fn()} />);
    expect(screen.getByTestId("notifications-pause-status")).toHaveTextContent(
      "Pause notifications"
    );
    expect(screen.queryByTestId("notifications-pause-resume")).toBeNull();
  });

  it("pauses for 1 hour", async () => {
    expect(await pick("hour")).toEqual(new Date(2026, 9, 17, 15, 30));
  });

  it("pauses until tomorrow 8:00", async () => {
    expect(await pick("tomorrow")).toEqual(new Date(2026, 9, 18, 8, 0));
  });

  it("pauses for 1 week", async () => {
    expect(await pick("week")).toEqual(new Date(2026, 9, 24, 14, 30));
  });

  it("while paused, says until when and resumes", async () => {
    const onChange = vi.fn();
    render(
      <PauseRow global={global(new Date(2026, 9, 18, 8, 0).toISOString())} onChange={onChange} />
    );
    expect(screen.getByTestId("notifications-pause-status")).toHaveTextContent(
      "Paused until tomorrow 8:00 AM"
    );
    expect(screen.queryByTestId("notifications-pause-open")).toBeNull();
    await userEvent.click(screen.getByTestId("notifications-pause-resume"));
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it("treats a pause that has ended as no pause", () => {
    render(
      <PauseRow global={global(new Date(2026, 9, 17, 9, 0).toISOString())} onChange={vi.fn()} />
    );
    expect(screen.getByTestId("notifications-pause-status")).toHaveTextContent(
      "Pause notifications"
    );
  });
});
