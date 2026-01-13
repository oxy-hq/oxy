import { useState, useEffect } from "react";
import { OxyProvider, type OxyConfig, OxySDK } from "@oxy-hq/sdk";
import ConfigForm from "./components/ConfigForm";
import PostMessageAuthModal from "./components/PostMessageAuthModal";
import AppContent from "./AppContent";
import "./App.css";

type AuthMode = "idle" | "postMessage" | "direct";

function App() {
  const [authMode, setAuthMode] = useState<AuthMode>("idle");
  const [showAuthModal, setShowAuthModal] = useState(false);
  const [config, setConfig] = useState<Partial<OxyConfig> | null>(null);
  const [configKey, setConfigKey] = useState(0);

  // Show postMessage modal on mount to simulate iframe auth
  useEffect(() => {
    // Simulate delay as if waiting for parent window
    const timer = setTimeout(() => {
      setShowAuthModal(true);
    }, 500);

    return () => clearTimeout(timer);
  }, []);

  const handlePostMessageAuth = (authConfig: OxyConfig) => {
    setConfig(authConfig);
    setAuthMode("postMessage");
    setShowAuthModal(false);
    setConfigKey((prev) => prev + 1);
  };

  const handleDirectConnect = (directConfig: OxyConfig) => {
    setConfig(directConfig);
    setAuthMode("direct");
    setConfigKey((prev) => prev + 1);
  };

  const handleCancelAuth = () => {
    setShowAuthModal(false);
    setAuthMode("idle");
  };

  const handleReady = (sdk: InstanceType<typeof OxySDK>) => {
    console.log("SDK ready:", sdk);
    console.log("Auth mode:", authMode);
  };

  const handleError = (err: Error) => {
    console.error("SDK error:", err);
  };

  return (
    <div className="app">
      <header className="header">
        <h1>🚀 Oxy SDK React Demo</h1>
        <p>Interactive demo with OxyProvider and Async PostMessage Auth</p>
        {authMode !== "idle" && (
          <span className="auth-badge">
            {authMode === "postMessage"
              ? "🔐 PostMessage Auth"
              : "🔌 Direct Auth"}
          </span>
        )}
      </header>

      <div className="container">
        {!config && (
          <>
            <ConfigForm onConnect={handleDirectConnect} connected={false} />
            <div className="empty-state">
              <p>Or wait for simulated postMessage authentication...</p>
            </div>
          </>
        )}

        {config && (
          <OxyProvider
            key={configKey}
            config={config}
            useAsync={authMode === "postMessage"}
            onReady={handleReady}
            onError={handleError}
          >
            <AppContent />
          </OxyProvider>
        )}
      </div>

      {showAuthModal && (
        <PostMessageAuthModal
          onAuth={handlePostMessageAuth}
          onCancel={handleCancelAuth}
        />
      )}
    </div>
  );
}

export default App;
