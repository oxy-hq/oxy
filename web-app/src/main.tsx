import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.tsx";
import { cn } from "./libs/shadcn/utils.ts";
import { initSentry } from "./sentry";
import useTheme from "./stores/useTheme.ts";

initSentry();

const queryClient = new QueryClient();

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
