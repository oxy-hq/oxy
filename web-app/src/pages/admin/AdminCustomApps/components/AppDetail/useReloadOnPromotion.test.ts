// @vitest-environment jsdom

import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useReloadOnPromotion } from "./useReloadOnPromotion";

/**
 * The Live Preview follows a Make Live / rollback: the refreshed app row's
 * `last_promoted_at` changes, and the frame remounts onto the new live build.
 */
describe("useReloadOnPromotion", () => {
  it("reloads when the live build moves, not on mount or re-render", () => {
    const reload = vi.fn();
    const { rerender } = renderHook(({ at }) => useReloadOnPromotion(at, reload), {
      initialProps: { at: "2026-10-16T08:00:00Z" as string | null | undefined }
    });
    rerender({ at: "2026-10-16T08:00:00Z" });
    expect(reload).not.toHaveBeenCalled();

    rerender({ at: "2026-10-16T09:30:00Z" });
    expect(reload).toHaveBeenCalledTimes(1);
    rerender({ at: "2026-10-16T09:30:00Z" });
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("does not treat the detail row's first arrival as a promotion", () => {
    const reload = vi.fn();
    const { rerender } = renderHook(({ at }) => useReloadOnPromotion(at, reload), {
      initialProps: { at: undefined as string | null | undefined }
    });
    rerender({ at: "2026-10-16T08:00:00Z" });
    expect(reload).not.toHaveBeenCalled();
    rerender({ at: "2026-10-16T10:00:00Z" });
    expect(reload).toHaveBeenCalledTimes(1);
  });
});
