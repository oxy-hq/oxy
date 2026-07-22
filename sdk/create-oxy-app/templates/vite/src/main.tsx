import { OxyAppProvider } from "@oxy-hq/sdk";
import { OxyShell } from "@oxy-hq/sdk/shell";
import "@oxy-hq/sdk/shell.css";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// `OxyShell` wraps the app in the Oxygen workspace chrome — the same icon
// rail + top bar the main web-app renders — so your app reads as part of
// the HQ. It bootstraps itself from the platform and renders your app
// unchromed when that data isn't available (e.g. plain `vite dev` with no
// oxy server). Remove the wrapper (and the shell.css import) if the app
// should own its entire viewport.
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <OxyAppProvider>
      <OxyShell>
        <App />
      </OxyShell>
    </OxyAppProvider>
  </React.StrictMode>,
);
