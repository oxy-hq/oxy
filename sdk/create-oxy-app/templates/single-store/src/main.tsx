import { OxyAppProvider } from "@oxy-hq/sdk";
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <OxyAppProvider>
      <App />
    </OxyAppProvider>
  </React.StrictMode>,
);
