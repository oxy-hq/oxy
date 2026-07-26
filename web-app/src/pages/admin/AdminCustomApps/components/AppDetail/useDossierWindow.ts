import { useCallback, useEffect, useRef } from "react";
import { toast } from "sonner";

/** Roughly a side column's worth of width, tall enough to read a build list. */
const WINDOW_FEATURES = "popup=yes,width=560,height=900";

interface DossierWindowOptions {
  /** Whether the dossier should currently live in its own window. */
  active: boolean;
  /** App-relative path the window renders. */
  url: string;
  /** Stable window name, so re-opening reuses the same OS window. */
  name: string;
  /**
   * Called when the window goes away for a reason this component didn't
   * choose — the operator closed it, or the browser blocked it. Must be
   * referentially stable, or the window reopens on every render.
   */
  onDismiss: () => void;
}

/**
 * Owns the lifecycle of the popped-out dossier window.
 *
 * A real `window.open` on a real route rather than a React portal into a popup
 * document: the dossier renders Radix overlays (the "Make this build live?"
 * confirmation, copy tooltips) which portal to *their own document's* body. A
 * portal-based popup would render those into the opener window instead, so the
 * confirmation for a click in the popup would appear behind it in the main
 * tab. A separate route gets its own React root, its own portal container, and
 * its own query cache — everything just works.
 */
export function useDossierWindow({ active, url, name, onDismiss }: DossierWindowOptions) {
  const windowRef = useRef<Window | null>(null);
  // The URL the popup was last pointed at, so a rail selection change navigates
  // the existing window instead of closing and reopening it.
  const navigatedRef = useRef<string | null>(null);
  // Read through a ref in the open effect below: the window opens at whatever
  // app is selected *at that moment*, and a later selection re-points it rather
  // than tearing it down and popping a new one.
  const urlRef = useRef(url);
  urlRef.current = url;

  useEffect(() => {
    if (!active) return;

    const popup = window.open(urlRef.current, name, WINDOW_FEATURES);
    if (!popup) {
      toast.error(
        "Your browser blocked the details window. Allow pop-ups for this site, then try again."
      );
      onDismiss();
      return;
    }

    windowRef.current = popup;
    navigatedRef.current = urlRef.current;
    popup.focus();

    // No "closed" event exists for a window we opened — polling is the only way
    // to notice the operator dismissing it, and to fold the dock back to a
    // placement that's actually visible.
    const timer = window.setInterval(() => {
      if (popup.closed) {
        window.clearInterval(timer);
        windowRef.current = null;
        onDismiss();
      }
    }, 500);

    return () => {
      window.clearInterval(timer);
      popup.close();
      windowRef.current = null;
      navigatedRef.current = null;
    };
  }, [active, name, onDismiss]);

  useEffect(() => {
    const popup = windowRef.current;
    if (!active || !popup || popup.closed || navigatedRef.current === url) return;
    navigatedRef.current = url;
    popup.location.replace(url);
  }, [active, url]);

  return useCallback(() => windowRef.current?.focus(), []);
}
