import { create } from "zustand";
import { persist } from "zustand/middleware";

interface ThemeState {
  theme: string;
  setTheme: (theme: string) => void;
}

const useTheme = create<ThemeState>()(
  persist(
    (set) => ({
      theme: "dark",
      setTheme: (theme: string) => set({ theme }),
    }),
    {
      name: "theme-storage",
    },
  ),
);

export default useTheme;
