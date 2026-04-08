import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.tsx";
import { initSentry } from "./sentry";
import useTheme from "./stores/useTheme.ts";

initSentry();

const queryClient = new QueryClient();

export const AppWrapper = () => {
  const { theme } = useTheme();

  useEffect(() => {
    document.body.classList.add(theme);
  }, [theme]);

  return (
    <div
      id='app-root'
      className={`root ${theme}`}
      lang='en'
      data-theme-variant='new'
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
