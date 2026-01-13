import { useState } from "react";
import type { OxyConfig } from "@oxy-hq/sdk";

interface ConfigFormProps {
  onConnect: (config: OxyConfig) => void;
  connected: boolean;
}

export default function ConfigForm({ onConnect, connected }: ConfigFormProps) {
  const [url, setUrl] = useState("http://localhost:3000/api");
  const [apiKey, setApiKey] = useState("");
  const [projectId, setProjectId] = useState(
    "00000000-0000-0000-0000-000000000000",
  ); // Default to zero uuid
  const [branch, setBranch] = useState("main");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const config: OxyConfig = {
      baseUrl: url,
      projectId,
      branch: branch || undefined,
    };

    // Only add apiKey if provided
    if (apiKey) {
      config.apiKey = apiKey;
    }

    onConnect(config);
  };

  return (
    <form onSubmit={handleSubmit} className="config-form">
      <h2>⚙️ Configuration</h2>

      <div className="form-grid">
        <div className="form-group">
          <label htmlFor="url">Oxy URL</label>
          <input
            id="url"
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://api.oxy.tech"
            required
            disabled={connected}
          />
        </div>

        <div className="form-group">
          <label htmlFor="apiKey">API Key (optional for local dev)</label>
          <input
            id="apiKey"
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="Your API key (leave empty for local)"
            disabled={connected}
          />
        </div>

        <div className="form-group">
          <label htmlFor="projectId">Project ID</label>
          <input
            id="projectId"
            type="text"
            value={projectId}
            onChange={(e) => setProjectId(e.target.value)}
            placeholder="your-project-uuid"
            required
            disabled={connected}
          />
        </div>

        <div className="form-group">
          <label htmlFor="branch">Branch (optional)</label>
          <input
            id="branch"
            type="text"
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder="main"
            disabled={connected}
          />
        </div>
      </div>

      <button
        type="submit"
        className="btn btn-primary btn-block"
        disabled={connected}
      >
        {connected ? "✓ Connected" : "🔌 Connect to Oxy"}
      </button>
    </form>
  );
}
