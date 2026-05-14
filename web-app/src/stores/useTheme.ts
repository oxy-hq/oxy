import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ThemeMode = "light" | "dark" | "system";
type ResolvedTheme = "light" | "dark";

interface ThemeState {
  mode: ThemeMode;
  theme: ResolvedTheme;
  setMode: (mode: ThemeMode) => void;
}

const SYSTEM_DARK_QUERY = "(prefers-color-scheme: dark)";

const getSystemTheme = (): ResolvedTheme => {
  if (typeof window === "undefined" || !window.matchMedia) {
    return "light";
  }
  return window.matchMedia(SYSTEM_DARK_QUERY).matches ? "dark" : "light";
};

const resolveTheme = (mode: ThemeMode, systemTheme: ResolvedTheme): ResolvedTheme =>
  mode === "system" ? systemTheme : mode;

const isThemeMode = (value: unknown): value is ThemeMode =>
  value === "light" || value === "dark" || value === "system";

const useTheme = create<ThemeState>()(
  persist(
    (set) => ({
      mode: "light",
      theme: "light",
      setMode: (mode) => {
        const theme = resolveTheme(mode, getSystemTheme());
        set({ mode, theme });
      }
    }),
    {
      name: "theme-storage",
      partialize: (state) => ({ mode: state.mode }),
      merge: (persisted, current) => {
        const p = persisted as { mode?: unknown } | null;
        const mode: ThemeMode = isThemeMode(p?.mode) ? p.mode : current.mode;
        return { ...current, mode, theme: resolveTheme(mode, getSystemTheme()) };
      }
    }
  )
);

export default useTheme;
