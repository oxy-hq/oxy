import { useCallback, useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { cn } from "@/libs/shadcn/utils";
import useAppDock from "@/stores/useAppDock";
import { DockHeader } from "./components/DockHeader";
import { useAppDockWidth } from "./useAppDockWidth";

/**
 * The docked custom app — a right-hand flex sibling of `<main>` that COMPACTS
 * the page (the same shape as the Ask dock) and renders one app in an
 * `<iframe>`.
 *
 * ## Why an iframe rather than a route
 *
 * A custom app is an independently published bundle with its own router,
 * its own chunks, and its own service worker. Rendering it in-place would mean
 * either loading a foreign bundle into the shell's JS context (no) or
 * navigating away from HQ (which is what the "open in a new tab" affordance is
 * for). An iframe is the only way to have the app *and* HQ on screen at once,
 * and it is same-origin — the app is served from `/customer-apps/…` on this
 * host — so cookies, storage, and the SDK's same-origin fetches all work
 * exactly as they do at the app's own URL.
 *
 * No `sandbox` attribute, for the reason the admin live-preview already
 * documents: the only token pair that would let a real app function
 * (`allow-same-origin` + `allow-scripts`) is specified as "effectively no
 * sandbox", so the attribute would be theatre. The trust boundary is that we
 * ship the bundle.
 *
 * ## Unmount, don't collapse
 *
 * The Ask dock stays mounted at width 0 so composer text survives a collapse.
 * This one unmounts, because the state it would be preserving lives inside a
 * frame we do not control and cannot restore selectively — a collapsed iframe
 * keeps running timers, polling, and SSE streams for an app nobody is looking
 * at. Closing means closing.
 */
export function AppDock() {
  const app = useAppDock((s) => s.app);
  const focus = useAppDock((s) => s.focus);
  const close = useAppDock((s) => s.close);
  const toggleFocus = useAppDock((s) => s.toggleFocus);
  const { width, isDesktop, dragging, minWidth, maxWidth, handleProps } = useAppDockWidth();

  // Bumping this remounts the iframe, which is the only reliable way to reload
  // a same-origin frame the user may have navigated deep into: assigning
  // `contentWindow.location` would work but pushes an entry onto the parent's
  // history in some browsers.
  const [reloadNonce, setReloadNonce] = useState(0);
  const frameRef = useRef<HTMLIFrameElement>(null);

  // The dock belongs to the page it was opened from. Navigating away closes it,
  // which is the opposite of the Ask dock's rule and deliberately so:
  //
  //  - Focus mode hides `<main>`. A dock that survived navigation would leave
  //    someone who clicked "Chat" in the rail looking at the same app, with the
  //    page they asked for rendered invisibly behind it.
  //  - An iframe that is open is an app that is running — polling, streaming,
  //    holding timers. Keeping it alive across a route the user moved on from
  //    spends their machine on something they are no longer looking at.
  //
  // The path is captured on the render where the dock first appears rather than
  // stored alongside the app, so `open()` stays a one-argument call from
  // wherever a card lives.
  const location = useLocation();
  const openedPath = useRef<string | null>(null);
  useEffect(() => {
    if (!app) {
      openedPath.current = null;
      return;
    }
    if (openedPath.current === null) {
      openedPath.current = location.pathname;
      return;
    }
    if (openedPath.current !== location.pathname) close();
  }, [app, location.pathname, close]);

  const onEscape = useCallback(
    (e: KeyboardEvent) => {
      // Only when the shell has focus. Escape inside the frame belongs to the
      // app — a dialog it opened should close before its host does.
      if (e.key === "Escape" && document.activeElement !== frameRef.current) close();
    },
    [close]
  );
  useEffect(() => {
    if (!app) return;
    window.addEventListener("keydown", onEscape);
    return () => window.removeEventListener("keydown", onEscape);
  }, [app, onEscape]);

  if (!app) return null;

  // In focus mode the shell hides `<main>` entirely and the dock is the only
  // flex child left, so it takes everything the rail and top bar don't — not
  // the user's remembered width, which is a split and would defeat the point.
  // The rail deliberately stays: it is the app switcher, and focus mode with no
  // way out of the app would be a trap rather than a focus.
  const dockWidth = !isDesktop || focus ? "100%" : width;

  return (
    <aside
      data-testid='app-dock'
      data-focus={focus ? "on" : "off"}
      style={{ width: dockWidth }}
      className={cn(
        "relative flex h-full shrink-0 flex-col overflow-hidden border-l bg-background",
        dragging ? "select-none" : "transition-[width] duration-150"
      )}
    >
      {isDesktop && !focus && (
        // biome-ignore lint/a11y/useSemanticElements: focusable resize separator, not a static divider
        <div
          role='separator'
          aria-orientation='vertical'
          aria-label={`Resize ${app.name} panel`}
          aria-valuenow={width}
          aria-valuemin={minWidth}
          aria-valuemax={maxWidth}
          tabIndex={0}
          data-testid='app-dock-resize'
          {...handleProps}
          className='absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize hover:bg-primary/20 focus-visible:bg-primary/30 focus-visible:outline-none'
        />
      )}

      <DockHeader
        app={app}
        focus={focus}
        onReload={() => setReloadNonce((n) => n + 1)}
        onToggleFocus={toggleFocus}
        onClose={close}
      />

      <iframe
        ref={frameRef}
        // `app.id` in the key so switching apps replaces the frame rather than
        // navigating the old one — otherwise the previous app's history stack
        // and its unload handlers come along.
        key={`${app.id}:${reloadNonce}`}
        src={app.url}
        title={app.name}
        className='min-h-0 flex-1 border-0 bg-background'
        // `allow` rather than `sandbox`: these are capabilities a dashboard
        // legitimately asks for and an iframe does not get by default.
        allow='clipboard-write; fullscreen; downloads'
      />
    </aside>
  );
}
