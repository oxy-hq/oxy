import { useState } from "react";
import type { OxyConfig } from "@oxy-hq/sdk";

interface PostMessageAuthModalProps {
  onAuth: (config: OxyConfig) => void;
  onCancel: () => void;
}

export default function PostMessageAuthModal({
  onAuth,
  onCancel,
}: PostMessageAuthModalProps) {
  const [url, setUrl] = useState("http://localhost:3000/api");
  const [apiKey, setApiKey] = useState("");
  const [projectId, setProjectId] = useState(
    "00000000-0000-0000-0000-000000000000",
  );
  const [branch, setBranch] = useState("main");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    const config: OxyConfig = {
      baseUrl: url,
      projectId,
      branch: branch || undefined,
    };

    if (apiKey) {
      config.apiKey = apiKey;
    }

    onAuth(config);
  };

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>🔐 Simulated PostMessage Authentication</h2>
          <p>
            This simulates a parent window providing authentication via
            postMessage
          </p>
        </div>

        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <label htmlFor="modal-url">Oxy URL</label>
            <input
              id="modal-url"
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://api.oxy.tech"
              required
            />
          </div>

          <div className="form-group">
            <label htmlFor="modal-apiKey">API Key</label>
            <input
              id="modal-apiKey"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="Your API key (optional for local)"
            />
          </div>

          <div className="form-group">
            <label htmlFor="modal-projectId">Project ID</label>
            <input
              id="modal-projectId"
              type="text"
              value={projectId}
              onChange={(e) => setProjectId(e.target.value)}
              placeholder="your-project-uuid"
              required
            />
          </div>

          <div className="form-group">
            <label htmlFor="modal-branch">Branch</label>
            <input
              id="modal-branch"
              type="text"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder="main"
            />
          </div>

          <div className="modal-actions">
            <button
              type="button"
              onClick={onCancel}
              className="btn btn-secondary"
            >
              Cancel
            </button>
            <button type="submit" className="btn btn-primary">
              Authenticate
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
