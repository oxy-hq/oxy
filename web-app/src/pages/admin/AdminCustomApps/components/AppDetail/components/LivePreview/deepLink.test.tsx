// @vitest-environment jsdom

import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CustomApp } from "@/types/apps";
import { LivePreview } from "./index";

/**
 * The deep-link sequence, at the level that can see it.
 *
 * `?preview=` is read on mount; the iframe fires `load` some time later. Every
 * other harness in this folder drives `handleLoad` first, so none of them can
 * observe the order that actually happens in a browser — which is where the
 * bug lived: the mount pass recorded a destination the frame had never gone to,
 * and the apply on first load then skipped it as "already there".
 *
 * Rendering the real component is the point. LivePreview has no query client or
 * router in its tree, so this costs a jsdom render and nothing else.
 */

const ORIGIN = "http://localhost:3000";
const BASE = "/customer-apps/poke-house/bookkeeping/";

const app = {
  id: "app-1",
  slug: "bookkeeping",
  name: "Bookkeeping",
  org_id: "org-1",
  org_slug: "poke-house",
  project_id: "p1",
  branch: "main",
  source_repo: "",
  status: "ready",
  url: `${ORIGIN}${BASE}`,
  published_at: "2026-08-01T00:00:00Z"
} as unknown as CustomApp;

/**
 * Stand in for the iframe's content window. The real one is unreachable in
 * jsdom (no navigation), so `LivePreview`'s `onLoad` is fired by hand with a
 * frame whose `contentWindow` is this.
 */
function fakeFrame(at: string) {
  let href = `${ORIGIN}${at}`;
  const replace = vi.fn((next: string) => {
    href = new URL(next, href).toString();
  });
  const win = {
    document,
    location: {
      get href() {
        return href;
      },
      get origin() {
        return ORIGIN;
      },
      get hash() {
        return new URL(href).hash;
      },
      replace,
      reload: vi.fn()
    },
    history: {
      state: null,
      pushState: vi.fn(),
      replaceState: vi.fn((_d: unknown, _t: unknown, url?: string | URL | null) => {
        if (url) href = new URL(String(url), href).toString();
      }),
      back: vi.fn(),
      forward: vi.fn(),
      go: vi.fn()
    },
    addEventListener: window.addEventListener.bind(window),
    dispatchEvent: window.dispatchEvent.bind(window),
    Event,
    HashChangeEvent,
    PopStateEvent
  };
  return { frame: { contentWindow: win } as unknown as HTMLIFrameElement, replace };
}

/** Fire the load event the way the browser would, once. */
const land = (container: HTMLElement, frame: HTMLIFrameElement) => {
  const iframe = container.querySelector("iframe");
  if (!iframe) throw new Error("no iframe rendered");
  Object.defineProperty(iframe, "contentWindow", {
    value: frame.contentWindow,
    configurable: true
  });
  iframe.dispatchEvent(new Event("load"));
};

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

beforeEach(() => {
  vi.restoreAllMocks();
});

describe("opening a shared ?preview= link", () => {
  /// THE regression. The mount effect runs while `frameRef` is still null, so
  /// the navigation cannot happen yet — and recording it anyway made the apply
  /// on first load early-return as "already there". The operator opened a
  /// shared link and got the app root.
  it("applies the path once the frame lands, not on mount", async () => {
    const onPathChange = vi.fn();
    const { container } = render(
      <LivePreview
        app={app}
        device='desktop'
        channel='published'
        nonce={0}
        path='/stores'
        onPathChange={onPathChange}
      />
    );

    // Mount has run; there is no frame, so nothing can have navigated.
    const { frame, replace } = fakeFrame(BASE);
    expect(replace).not.toHaveBeenCalled();

    land(container, frame);

    await waitFor(() => {
      expect(replace).toHaveBeenCalledWith(`${ORIGIN}${BASE.slice(0, -1)}/stores`);
    });
  });

  /// The second half of the same bug, and the one this round made visible: with
  /// the destination wrongly recorded, the bundle root got published and
  /// `writeAppViewState` dropped `?preview` as its default — so the link the
  /// operator was sent vanished from the address bar in front of them.
  it("does not publish the document it is passing through", async () => {
    const onPathChange = vi.fn();
    const { container } = render(
      <LivePreview
        app={app}
        device='desktop'
        channel='published'
        nonce={0}
        path='/stores'
        onPathChange={onPathChange}
      />
    );

    land(container, fakeFrame(BASE).frame);

    // The root is where the frame is passing through, not where it was asked
    // to be. Publishing "/" is what erased the param.
    await waitFor(() => expect(onPathChange).not.toHaveBeenCalledWith("/"));
  });

  /// A fragment-only deep link is a same-document move: no load event follows,
  /// so the handoff's "a new document landed" exit never fires. Arming it there
  /// would latch on any path that does not round-trip byte-identically — a
  /// space in a fragment comes back percent-encoded — and every later report
  /// would be dropped for the life of the component. Nothing to pass through
  /// means nothing to suppress.
  it("does not latch the handoff on a fragment-only link", async () => {
    const onPathChange = vi.fn();
    const { container } = render(
      <LivePreview
        app={app}
        device='desktop'
        channel='published'
        nonce={0}
        path='/#a b'
        onPathChange={onPathChange}
      />
    );

    land(container, fakeFrame(BASE).frame);

    // `/#a b` normalises to `/#a%20b`, so an armed handoff would never see its
    // exact match and would suppress this — and everything after it.
    await waitFor(() => expect(onPathChange).toHaveBeenCalledWith("/#a%20b"));
  });

  it("still follows the app once no link is in flight", async () => {
    const onPathChange = vi.fn();
    const { container } = render(
      <LivePreview
        app={app}
        device='desktop'
        channel='published'
        nonce={0}
        path={null}
        onPathChange={onPathChange}
      />
    );

    land(container, fakeFrame(`${BASE}stores`).frame);

    await waitFor(() => expect(onPathChange).toHaveBeenCalledWith("/stores"));
  });
});
