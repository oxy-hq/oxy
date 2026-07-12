import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.tsx";
import { cn } from "./libs/shadcn/utils.ts";
import { isIdeUnavailableError } from "./libs/utils/ideHealth.ts";
import { initSentry } from "./sentry";
import useTheme from "./stores/useTheme.ts";

// After a deploy, chunk hashes rotate; a still-open client that lazily imports
// a route whose chunk no longer exists white-screens (issues #2697 / #2699).
// Vite fires `vite:preloadError` in that case — reload once to pull the fresh
// index + chunk graph, guarded by a session flag so a genuinely broken import
// can't reload-loop.
const PRELOAD_RELOAD_KEY = "vite-preload-reloaded";
window.addEventListener("vite:preloadError", () => {
  if (sessionStorage.getItem(PRELOAD_RELOAD_KEY)) return;
  sessionStorage.setItem(PRELOAD_RELOAD_KEY, "1");
  window.location.reload();
});
// Once a load completes cleanly, drop the one-shot guard so a later deploy can
// self-heal again.
window.addEventListener("load", () => {
  sessionStorage.removeItem(PRELOAD_RELOAD_KEY);
});

initSentry();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Don't retry a request that failed because the ide (developer
      // environment) is unreachable — it can't succeed until the ide is back,
      // so retrying only storms the proxy and re-trips the global banner.
      // Homepage readiness checks (modeling, github-setup, etc.) that hit
      // IdeOnly routes thus fail once, quietly, instead of three times. Other
      // errors keep React Query's default retry behaviour.
      retry: (failureCount, error) => !isIdeUnavailableError(error) && failureCount < 3
    }
  }
});

export const AppWrapper = () => {
  const theme = useTheme((state) => state.theme);

  useEffect(() => {
    document.body.classList.toggle("dark", theme === "dark");
    return () => document.body.classList.remove("dark");
  }, [theme]);

  return (
    <div
      id='app-root'
      className={cn("root font-inter", theme === "dark" && "dark")}
      lang='en'
      data-theme={theme}
    >
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </div>
  );
};

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppWrapper />
  </StrictMode>
);
