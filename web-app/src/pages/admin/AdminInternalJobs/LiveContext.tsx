import { createContext, type ReactNode, useContext, useMemo, useState } from "react";

/**
 * Page-wide pause state for the Internal Jobs console. The
 * LiveIndicator owns the toggle in the topbar; every data-fetching
 * sub-component reads `paused` here and passes it to its hook so the
 * 5s background polls go quiet in unison.
 *
 * Kept deliberately tiny — no analytics, no persistence, no other
 * state. If a second piece of cross-component state arrives, promote
 * to a Zustand slice instead of growing this context.
 */
interface LiveState {
  paused: boolean;
  togglePaused: () => void;
}

const LiveCtx = createContext<LiveState | null>(null);

export const LiveProvider = ({ children }: { children: ReactNode }) => {
  const [paused, setPaused] = useState(false);
  const value = useMemo<LiveState>(
    () => ({ paused, togglePaused: () => setPaused((p) => !p) }),
    [paused]
  );
  return <LiveCtx.Provider value={value}>{children}</LiveCtx.Provider>;
};

export const useLive = (): LiveState => {
  const ctx = useContext(LiveCtx);
  if (!ctx) throw new Error("useLive must be used inside LiveProvider");
  return ctx;
};
