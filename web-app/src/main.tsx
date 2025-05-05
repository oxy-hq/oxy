import { StrictMode, useEffect } from "react";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRoot } from "react-dom/client";
import useTheme from "./stores/useTheme.ts";
import App from "./App.tsx";

const queryClient = new QueryClient();

export const AppWrapper = () => {
  const { theme } = useTheme();

  useEffect(() => {
    document.body.classList.add(theme);
  }, [theme]);

  return (
    <div
      id="app-root"
      className={`root ${theme}`}
      lang="en"
      data-theme-variant="new"
      data-theme={theme}
    >
      <StrictMode>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </StrictMode>
    </div>
  );
};

createRoot(document.getElementById("root")!).render(<AppWrapper />); // Render AppWrapper
