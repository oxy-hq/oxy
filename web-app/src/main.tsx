import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.tsx";
import { cn } from "./libs/shadcn/utils.ts";
import { isIdeUnavailableError } from "./libs/utils/ideHealth.ts";
import { initSentry } from "./sentry";
import useTheme from "./stores/useTheme.ts";

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
