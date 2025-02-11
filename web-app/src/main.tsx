import { StrictMode } from "react";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRoot } from "react-dom/client";

import App from "./App.tsx";

import "./fonts/font-face.css";
import "./index.css";

import Toaster from "./components/ui/Toast/Toaster.tsx";
import useTheme from "./stores/useTheme.ts";

const queryClient = new QueryClient();

const AppWrapper = () => {
  const { theme } = useTheme();

  return (
    <div
      id="app-root"
      className={`root`}
      lang="en"
      data-theme-variant="new"
      data-theme={theme}
    >
      <StrictMode>
        <QueryClientProvider client={queryClient}>
          <App />

          <Toaster />
        </QueryClientProvider>
      </StrictMode>
    </div>
  );
};

createRoot(document.getElementById("root")!).render(<AppWrapper />); // Render AppWrapper
