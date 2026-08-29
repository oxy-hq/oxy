// @vitest-environment jsdom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { usePreviewHistory } from "./usePreviewHistory";

/**
 * The preview chrome's own controls, exercised through the hook.
 *
 * Reload is here rather than in `installGuard.test.tsx` because it is the
 * hook's, not the guard's — and it was wrong in exactly the way the guard's
 * click handling was: plausible code, no harness, dead control.
 */

const HERE = "https://app.oxygen-hq.com/customer-apps/poke-house/bookkeeping/";

// Every hook here installs a guard on the ONE jsdom document this file gets.
// Unmounting is what drops it — an abandoned guard intercepts the next test's
// click and `preventDefault`s it before that test's own guard sees it, which
// reads as "the navigation silently did nothing".
afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

function frameAt(initial: string) {
  let href = initial;
  const reload = vi.fn();
  const replace = vi.fn();
  const win = {
    document,
    location: {
      get href() {
        return href;
      },
      get origin() {
        return new URL(href).origin;
      },
      get hash() {
        return new URL(href).hash;
      },
      reload,
      replace
    },
    history: {
      state: null,
      pushState: vi.fn(),
      // Moves the stub's location, the way the real one does — without this a
      // fragment click leaves `href` behind and the stack records the old URL.
      replaceState: vi.fn((_d: unknown, _t: unknown, url?: string | URL | null) => {
        if (url) href = new URL(String(url), href).toString();
      }),
      back: vi.fn(),
      forward: vi.fn(),
      go: vi.fn()
    },
    addEventListener: window.addEventListener.bind(window),
    dispatchEvent: window.dispatchEvent.bind(window),
    // The typed constructors, so this drives the same branch production does —
    // `announceSameDocumentMove` falls back to a bare `Event` only where the
    // realm lacks them, and a harness that always took the fallback would be
    // covering the degradation instead of the path.
    Event,
    HashChangeEvent,
    PopStateEvent
  };
  return { frame: { contentWindow: win } as unknown as HTMLIFrameElement, reload, replace };
}

describe("the preview's reload control", () => {
  /// THE regression. `navigate(currentEntry())` looks like a reload and is not:
  /// the target is by definition where the frame already is, so
  /// `isSameDocument` is unconditionally true, the fragment branch runs, and
  /// the button does a `replaceState` to the same URL plus a synthetic
  /// `popstate`. No network. A dead control beside the toolbar's working one.
  it("re-fetches the document instead of replaying the current entry", () => {
    const { result } = renderHook(() => usePreviewHistory());
    const { frame, reload, replace } = frameAt(HERE);

    act(() => result.current.handleLoad(frame));
    act(() => result.current.reload());

    expect(reload).toHaveBeenCalledTimes(1);
    // Neither of the two things the old shape did instead.
    expect(replace).not.toHaveBeenCalled();
  });

  it("is a no-op before the frame has loaded, rather than throwing", () => {
    const { result } = renderHook(() => usePreviewHistory());
    expect(() => act(() => result.current.reload())).not.toThrow();
  });

  /// Two fast clicks on Back have to walk two entries. The cursor lives in
  /// state, so a `step` that read the rendered value would recompute the same
  /// move twice; the ref mirror is what makes the second click see the first.
  it("walks two entries on two successive Back calls", () => {
    const { result } = renderHook(() => usePreviewHistory());
    const a = `${HERE}a`;
    const b = `${HERE}b`;
    const c = `${HERE}c`;

    act(() => result.current.handleLoad(frameAt(a).frame));
    act(() => result.current.handleLoad(frameAt(b).frame));
    act(() => result.current.handleLoad(frameAt(c).frame));
    expect(result.current.url).toBe(c);

    act(() => {
      result.current.back();
      result.current.back();
    });
    expect(result.current.url).toBe(a);
  });
});

describe("a fragment click, through the stack rather than the mock", () => {
  /// THE regression, and the reason it needs asserting HERE. Reported as a
  /// push, the fragment becomes a new entry and Back can return to where the
  /// operator was. Announced first instead, the synthetic `popstate` reports a
  /// *replace* that overwrites the current entry, after which the push is a
  /// no-op against a stack already on that URL — net effect, the replace the
  /// push exists to avoid.
  ///
  /// `installGuard.test.tsx` asserts `onNavigate` was called with "push" and
  /// passes either way, because both calls happen; only their order differs.
  /// The stack is what can tell them apart.
  it("leaves the previous fragment reachable by Back", () => {
    const { result } = renderHook(() => usePreviewHistory());
    const { frame } = frameAt(HERE);

    act(() => result.current.handleLoad(frame));
    expect(result.current.canBack).toBe(false);

    const a = document.createElement("a");
    a.href = `${HERE}#totals`;
    document.body.append(a);
    act(() => {
      a.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    });

    expect(result.current.url).toBe(`${HERE}#totals`);
    expect(result.current.canBack).toBe(true);

    act(() => result.current.back());
    expect(result.current.url).toBe(HERE);
  });
});
