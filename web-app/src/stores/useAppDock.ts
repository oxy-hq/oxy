import { create } from "zustand";
import type { CustomAppSummary } from "@/types/apps";
import useAskDock from "./useAskDock";

/** Persisted so returning to HQ doesn't undo the user's last focus choice. */
const FOCUS_KEY = "oxy:app-dock-focus";

const loadFocus = () => {
  try {
    return localStorage.getItem(FOCUS_KEY) !== "0";
  } catch {
    // Private-mode Safari and storage-blocked browsers throw on read.
    return true;
  }
};

const saveFocus = (focus: boolean) => {
  try {
    localStorage.setItem(FOCUS_KEY, focus ? "1" : "0");
  } catch {
    /* a lost preference is not worth an error boundary */
  }
};

interface AppDockState {
  /** The app currently docked, or `null` when the dock is closed. */
  app: CustomAppSummary | null;
  /**
   * Focus mode: the shell's chrome (icon rail, top bar) gets out of the way and
   * the dock takes most of the viewport, so the app — not the shell around
   * it — is what the user is looking at. On by default; the user can turn it
   * off to keep the shell visible beside the app, and that choice persists.
   */
  focus: boolean;
  open: (app: CustomAppSummary) => void;
  close: () => void;
  toggleFocus: () => void;
}

/**
 * The docked custom app — a right-hand pane that renders one app inside the
 * workspace shell instead of navigating away from it.
 *
 * Separate from `useAskDock` rather than a mode of it, because the two are
 * different objects with different lifetimes: the Ask dock deliberately stays
 * mounted across collapses so composer text and a live thread survive, while an
 * app dock holds an `<iframe>` whose whole state is the app's own and is
 * expected to reset when you close it. Modelling both in one store would mean
 * one of them getting the wrong retention.
 *
 * They are mutually exclusive on screen: two docks would leave the page with no
 * page. Opening an app closes Ask, which loses nothing — the Ask dock's own
 * `close()` is the state-preserving one.
 *
 * ## Why a store at all
 *
 * `web-app/CLAUDE.md` gates new Zustand stores on explicit approval, which this
 * one has (2026-08-23, on PR #2983). Recorded here rather than only in the PR
 * thread because the gate is re-checked on every review, and the argument is
 * short: the three consumers — `AppCard`, `AppDock`, `WorkspaceShell` — are not
 * in a parent/child line. Lifting the state into `WorkspaceShell` instead would
 * mean threading an `onOpenApp` callback down through the launcher grid, which
 * has no interest in the app dock and would carry the prop purely as transit.
 */
const useAppDock = create<AppDockState>()((set, get) => ({
  app: null,
  focus: loadFocus(),

  open: (app) => {
    useAskDock.getState().close();
    set({ app });
  },
  close: () => set({ app: null }),
  toggleFocus: () => {
    const focus = !get().focus;
    saveFocus(focus);
    set({ focus });
  }
}));

export default useAppDock;
