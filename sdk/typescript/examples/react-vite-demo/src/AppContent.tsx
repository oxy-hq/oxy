import { useState } from "react";
import { DataContainer, useOxy } from "@oxy-hq/sdk";
import AppList from "./components/AppList";
import AppDataView from "./components/AppDataView";
import "./App.css";

interface AppItem {
  name: string;
  path: string;
}

type Status = "idle" | "loading" | "success" | "error";

export default function AppContent() {
  const { sdk, isLoading: sdkLoading, error: sdkError } = useOxy();
  const [apps, setApps] = useState<AppItem[]>([]);
  const [selectedApp, setSelectedApp] = useState<AppItem | null>(null);
  const [appData, setAppData] = useState<DataContainer | null>(null);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string>("");

  const handleSelectApp = async (app: AppItem) => {
    if (!sdk) return;

    setSelectedApp(app);
    setStatus("loading");
    setError("");
    setAppData(null);

    try {
      const data = await sdk.loadAppData(app.path);
      setAppData(data);
      setStatus("success");
    } catch (err) {
      setError((err as Error).message);
      setStatus("error");
    }
  };

  const handleRunApp = async () => {
    if (!sdk || !selectedApp) return;

    setStatus("loading");
    setError("");

    try {
      const result = await sdk.getClient().runApp(selectedApp.path);

      if (result.error) {
        setError(result.error);
        setStatus("error");
      } else {
        setAppData(result.data);
        setStatus("success");
      }
    } catch (err) {
      setError((err as Error).message);
      setStatus("error");
    }
  };

  const handleListApps = async () => {
    if (!sdk) return;

    setStatus("loading");
    setError("");

    try {
      const appList = await sdk.getClient().listApps();
      setApps(appList);
      setStatus("success");
    } catch (err) {
      setError((err as Error).message);
      setStatus("error");
    }
  };

  if (sdkLoading) {
    return (
      <div className="app">
        <header className="header">
          <h1>🚀 Oxy SDK React Demo</h1>
          <p>Interactive demo of the Oxy TypeScript SDK with React + Vite</p>
        </header>
        <div className="container">
          <div className="alert alert-loading">
            <div className="spinner"></div>
            Initializing SDK...
          </div>
        </div>
      </div>
    );
  }

  if (sdkError) {
    return (
      <div className="app">
        <header className="header">
          <h1>🚀 Oxy SDK React Demo</h1>
          <p>Interactive demo of the Oxy TypeScript SDK with React + Vite</p>
        </header>
        <div className="container">
          <div className="alert alert-error">
            <strong>SDK Initialization Error:</strong> {sdkError.message}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="container">
      {error && (
        <div className="alert alert-error">
          <strong>Error:</strong> {error}
        </div>
      )}

      {status === "loading" && (
        <div className="alert alert-loading">
          <div className="spinner"></div>
          Loading...
        </div>
      )}

      <div className="section">
        <div className="section-header">
          <h2>Apps</h2>
          <button onClick={handleListApps} className="btn btn-primary">
            📋 {apps.length > 0 ? "Refresh Apps" : "List Apps"}
          </button>
        </div>
        <AppList
          apps={apps}
          selectedApp={selectedApp}
          onSelectApp={handleSelectApp}
        />
      </div>

      {selectedApp && (
        <div className="section">
          <div className="section-header">
            <h2>{selectedApp.name}</h2>
            <button onClick={handleRunApp} className="btn btn-secondary">
              ▶️ Run App
            </button>
          </div>
          <AppDataView appData={appData} />
        </div>
      )}
    </div>
  );
}
