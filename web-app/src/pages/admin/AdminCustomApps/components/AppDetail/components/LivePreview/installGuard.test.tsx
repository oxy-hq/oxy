// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installGuard } from "./usePreviewHistory";

/**
 * The guard's *wiring*, driven against a real document.
 *
 * `previewHistory.test.ts` covers the predicates; this covers what happens when
 * a click actually travels through the DOM to reach them — which is where the
 * first version's bug lived. It cancelled the browser's navigation in capture
 * phase and deferred its own replacement to a microtask that then read the flag
 * *it* had just set, so every intercepted link did nothing at all. Every
 * predicate involved was green.
 */

const ORIGIN = "https://app.oxygen-hq.com";
const HERE = `${ORIGIN}/customer-apps/poke-house/bookkeeping/`;

/** A stand-in for the framed window: a real jsdom document (so events really
 *  propagate) with the navigation surface stubbed so we can see what was
 *  called instead of asking jsdom to navigate. */
function makeFrameWindow(href = HERE) {
  const replace = vi.fn();
  const reload = vi.fn();
  const location = {
    get href() {
      return href;
    },
    origin: ORIGIN,
    get hash() {
      return new URL(href).hash;
    },
    replace,
    reload
  };
  const replaceState = vi.fn((_d: unknown, _t: unknown, url?: string | URL | null) => {
    if (url) href = new URL(String(url), href).toString();
  });
  const win = {
    document,
    location,
    history: {
      state: null,
      replaceState,
      pushState: vi.fn(),
      back: vi.fn(),
      forward: vi.fn(),
      go: vi.fn()
    },
    addEventListener: window.addEventListener.bind(window),
    dispatchEvent: window.dispatchEvent.bind(window),
    Event,
    HashChangeEvent,
    PopStateEvent
  } as unknown as Window;
  return { win, replace, reload, replaceState };
}

const anchorAt = (href: string, attrs: Record<string, string> = {}) => {
  const a = document.createElement("a");
  a.href = href;
  for (const [k, v] of Object.entries(attrs)) a.setAttribute(k, v);
  document.body.append(a);
  return a;
};

const click = (el: Element, init: MouseEventInit = {}) =>
  el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ...init }));

let hooks: {
  onNavigate: ReturnType<typeof vi.fn>;
  onTraverse: ReturnType<typeof vi.fn>;
  onReload: ReturnType<typeof vi.fn>;
};
// jsdom hands the whole file one `document`, so a guard left installed by an
// earlier case keeps listening: its `preventDefault` would set the flag the
// next case's guard checks, and every later assertion would pass or fail for
// the wrong reason. Dispose between cases.
let dispose: (() => void) | null = null;
const guard = (win: Window) => {
  dispose = installGuard(win, hooks);
};

beforeEach(() => {
  document.body.innerHTML = "";
  hooks = { onNavigate: vi.fn(), onTraverse: vi.fn(), onReload: vi.fn() };
});
afterEach(() => {
  dispose?.();
  dispose = null;
  document.body.innerHTML = "";
});

describe("intercepting a link click", () => {
  /// THE regression. A plain unhandled link inside the preview must actually
  /// go somewhere: the default navigation is cancelled (it would add a joint
  /// history entry) and replaced with `location.replace`, which does not.
  /// The first version cancelled and did not replace, leaving the link dead.
  it("cancels the default navigation AND performs the replacement", () => {
    const { win, replace } = makeFrameWindow();
    guard(win);

    const a = anchorAt(`${ORIGIN}/customer-apps/poke-house/bookkeeping/stores`);
    const notCancelled = click(a);

    expect(notCancelled).toBe(false); // preventDefault ran
    expect(replace).toHaveBeenCalledWith(`${ORIGIN}/customer-apps/poke-house/bookkeeping/stores`);
  });

  /// An app that handles its own links keeps them. Its `pushState` is already
  /// neutralised, so there is nothing left to intercept — and stealing the
  /// click would navigate the document out from under its router.
  it("defers to an app that claimed the click", () => {
    const { win, replace } = makeFrameWindow();
    guard(win);

    // A router's delegated handler: on a container, bubble phase, which is
    // where React attaches. It runs before `document` sees the event.
    const container = document.createElement("div");
    document.body.append(container);
    container.addEventListener("click", (e) => e.preventDefault());
    const a = document.createElement("a");
    a.href = `${ORIGIN}/customer-apps/poke-house/bookkeeping/stores`;
    container.append(a);

    click(a);
    expect(replace).not.toHaveBeenCalled();
  });

  it("leaves the gestures that open somewhere else", () => {
    const { win, replace } = makeFrameWindow();
    guard(win);

    click(anchorAt(`${HERE}stores`, { target: "_blank" }));
    click(anchorAt(`${HERE}report.csv`, { download: "" }));
    click(anchorAt("https://example.com/"));
    click(anchorAt(`${HERE}stores`), { metaKey: true });
    click(anchorAt(`${HERE}stores`), { button: 1 });

    expect(replace).not.toHaveBeenCalled();
  });

  /// A fragment link must not reload the document — but `replaceState` is
  /// silent, so the guard owes the scroll the cancelled navigation would have
  /// done, plus the events a router listens for.
  it("absorbs a fragment link without reloading, and still scrolls", () => {
    const { win, replace, replaceState } = makeFrameWindow();
    guard(win);

    const heading = document.createElement("h2");
    heading.id = "totals";
    heading.scrollIntoView = vi.fn();
    document.body.append(heading);

    const onHashChange = vi.fn();
    const onPopState = vi.fn();
    window.addEventListener("hashchange", onHashChange);
    window.addEventListener("popstate", onPopState);

    click(anchorAt(`${HERE}#totals`));

    expect(replace).not.toHaveBeenCalled(); // no document reload
    // The underlying `replaceState` still runs — that is how the URL moves
    // without a joint entry. What changed is which one is called: the captured
    // original, so the report below can say "push".
    expect(replaceState).toHaveBeenCalled();
    expect(heading.scrollIntoView).toHaveBeenCalled();
    expect(onHashChange).toHaveBeenCalled();
    // A BrowserRouter listens for popstate and never for hashchange, so
    // without this the synthetic hashchange reaches nothing.
    expect(onPopState).toHaveBeenCalled();
    // Typed, with the payload a real navigation would have carried — a
    // listener reading `newURL` must not get `undefined`.
    expect(onHashChange.mock.calls[0][0].newURL).toBe(`${HERE}#totals`);

    // The browser PUSHES for a fragment navigation. Going through the patched
    // `replaceState` would report "replace" and overwrite the current entry,
    // so the preview's own Back could not return to the previous fragment.
    expect(hooks.onNavigate).toHaveBeenCalledWith(`${HERE}#totals`, "push");

    window.removeEventListener("hashchange", onHashChange);
    window.removeEventListener("popstate", onPopState);
  });
});

describe("the framed app's own history calls", () => {
  it("turns pushState into replaceState and reports a push", () => {
    const { win, replaceState } = makeFrameWindow();
    guard(win);

    win.history.pushState(null, "", `${HERE}stores`);

    expect(replaceState).toHaveBeenCalled();
    expect(hooks.onNavigate).toHaveBeenCalledWith(`${HERE}stores`, "push");
  });

  /// The other half of the same problem. With the frame contributing no joint
  /// entries, an in-app Back button calling `history.back()` would traverse to
  /// the nearest entry — the ADMIN CONSOLE's — and navigate the operator off
  /// the page. It has to reach the preview's own stack instead.
  it("routes back/forward/go into the preview's stack, not the joint one", () => {
    const { win } = makeFrameWindow();
    guard(win);

    win.history.back();
    win.history.forward();
    win.history.go(-2);

    expect(hooks.onTraverse.mock.calls).toEqual([[-1], [1], [-2]]);
  });

  /// `go()` and `go(0)` mean reload in the platform. Routed into the cursor
  /// they were `moveCursor(s, 0)` — a silent no-op.
  it("treats go(0) as a reload rather than a zero-delta move", () => {
    const { win } = makeFrameWindow();
    guard(win);

    win.history.go(0);
    win.history.go();

    expect(hooks.onTraverse).not.toHaveBeenCalled();
    expect(hooks.onReload).toHaveBeenCalledTimes(2);
  });

  /// The disposer exists so one document can be guarded twice. Aborting the
  /// listeners is only half of that — the patched methods keep pointing at the
  /// old closure until they are put back.
  it("restores the history object on dispose", () => {
    const { win } = makeFrameWindow();
    const before = win.history.pushState;
    const off = installGuard(win, hooks);
    expect(win.history.pushState).not.toBe(before);

    off();
    expect(win.history.pushState).toBe(before);
    win.history.pushState(null, "", `${HERE}stores`);
    expect(hooks.onNavigate).not.toHaveBeenCalled();
  });

  it("survives a reporting hook that throws", () => {
    // Reporting is our concern; the app's navigation is the app's. A crash in
    // ours must not become a crash in theirs.
    const { win } = makeFrameWindow();
    dispose = installGuard(win, {
      onNavigate: () => {
        throw new Error("boom");
      },
      onTraverse: vi.fn(),
      onReload: vi.fn()
    });

    expect(() => win.history.pushState(null, "", `${HERE}stores`)).not.toThrow();
  });
});
