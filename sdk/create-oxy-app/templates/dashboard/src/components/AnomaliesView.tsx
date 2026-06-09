// Anomalies panel — exercises `sdk.anomalies` end-to-end:
// list / scan / acknowledge / dismiss / explain. Pairs with the metric
// tree above so the user can see anomalies the detector has flagged.

import { useEffect, useState } from "react";
import { type Anomaly, useOxy } from "@oxy-hq/sdk";

type Status = "idle" | "loading" | "success" | "error";

function severityColor(sev: string): string {
  switch (sev) {
    case "high":
      return "#ef4444";
    case "medium":
      return "#f59e0b";
    case "low":
    default:
      return "#3b82f6";
  }
}

function statusBadgeColor(status: string): { bg: string; fg: string } {
  switch (status) {
    case "acknowledged":
      return { bg: "#fef3c7", fg: "#92400e" };
    case "dismissed":
      return { bg: "#e5e7eb", fg: "#374151" };
    case "new":
    default:
      return { bg: "#dbeafe", fg: "#1e40af" };
  }
}

function formatNumber(n: number): string {
  if (!Number.isFinite(n)) return String(n);
  const abs = Math.abs(n);
  if (abs >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  if (abs >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  if (abs >= 1) return n.toFixed(2);
  return n.toFixed(4);
}

export default function AnomaliesView() {
  const { sdk } = useOxy();
  const [anomalies, setAnomalies] = useState<Anomaly[]>([]);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string>("");
  const [filter, setFilter] = useState<"all" | "new" | "acknowledged" | "dismissed">("new");
  const [asOf, setAsOf] = useState<string>("2025-12-15");
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = async (statusFilter: typeof filter = filter) => {
    if (!sdk) return;
    setStatus("loading");
    setError("");
    try {
      const opts = statusFilter === "all" ? {} : { status: statusFilter };
      const result = await sdk.anomalies.list(opts);
      setAnomalies(result.anomalies);
      setStatus("success");
    } catch (e) {
      setError((e as Error).message);
      setStatus("error");
    }
  };

  // Load anomalies on mount + whenever the filter changes.
  useEffect(() => {
    void refresh(filter);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filter, sdk]);

  const runScan = async () => {
    if (!sdk) return;
    setStatus("loading");
    setError("");
    try {
      const result = await sdk.anomalies.scan(asOf ? { as_of: asOf } : {});
      // Refresh after the scan so the inbox reflects new rows.
      await refresh(filter);
      setStatus("success");
      // Surface scan counts as a non-blocking message via `error` state —
      // it's the only "alert"-style slot we have without adding state.
      setError(
        `Scan done: ${result.monitors_scanned} monitor(s), ${result.anomalies_persisted} anomaly row(s) upserted${
          result.monitors_failed ? `, ${result.monitors_failed} failed` : ""
        }.`
      );
    } catch (e) {
      setError((e as Error).message);
      setStatus("error");
    }
  };

  const updateStatus = async (id: string, next: "new" | "acknowledged" | "dismissed") => {
    if (!sdk) return;
    setBusyId(id);
    try {
      await sdk.anomalies.updateStatus(id, next);
      await refresh(filter);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="section">
      <div className="section-header">
        <h2>Anomalies</h2>
        <div style={{ display: "flex", gap: "0.5rem", alignItems: "center", flexWrap: "wrap" }}>
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value as typeof filter)}
            style={{
              padding: "0.4rem 0.6rem",
              border: "1px solid #ccc",
              borderRadius: 4,
              fontSize: "0.85rem",
            }}
          >
            <option value="new">new</option>
            <option value="acknowledged">acknowledged</option>
            <option value="dismissed">dismissed</option>
            <option value="all">all</option>
          </select>
          <input
            type="date"
            value={asOf}
            onChange={(e) => setAsOf(e.target.value)}
            title="Reference 'now' date for the scan"
            style={{
              padding: "0.4rem 0.6rem",
              border: "1px solid #ccc",
              borderRadius: 4,
              fontSize: "0.85rem",
            }}
          />
          <button onClick={runScan} className="btn btn-secondary" disabled={status === "loading"}>
            🔄 Scan
          </button>
          <button
            onClick={() => refresh(filter)}
            className="btn btn-primary"
            disabled={status === "loading"}
          >
            ↻ Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="alert alert-error">
          <strong>Info:</strong> {error}
        </div>
      )}
      {status === "loading" && (
        <div className="alert alert-loading">
          <div className="spinner"></div>
          Loading anomalies...
        </div>
      )}

      {status !== "loading" && anomalies.length === 0 && (
        <div className="empty-state">
          <p>
            No {filter === "all" ? "" : filter} anomalies. Click <strong>Scan</strong> to run the
            detector against the reference date.
          </p>
        </div>
      )}

      {anomalies.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
          {anomalies.map((a) => {
            const badge = statusBadgeColor(a.status);
            const deviation = a.observed - a.expected;
            return (
              <div
                key={a.id}
                style={{
                  border: "1px solid #e5e7eb",
                  borderLeft: `4px solid ${severityColor(a.severity)}`,
                  borderRadius: 6,
                  padding: "0.75rem 1rem",
                  background: "#fff",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", gap: "0.5rem", flexWrap: "wrap" }}>
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", flexWrap: "wrap" }}>
                      <span style={{ fontWeight: 600 }}>{a.label ?? a.measure}</span>
                      <span
                        style={{
                          fontSize: "0.7rem",
                          padding: "2px 8px",
                          borderRadius: 8,
                          background: badge.bg,
                          color: badge.fg,
                          fontWeight: 600,
                          textTransform: "uppercase",
                          letterSpacing: 0.4,
                        }}
                      >
                        {a.status}
                      </span>
                      <span
                        style={{
                          fontSize: "0.7rem",
                          padding: "2px 8px",
                          borderRadius: 8,
                          background: severityColor(a.severity),
                          color: "white",
                          fontWeight: 600,
                          textTransform: "uppercase",
                          letterSpacing: 0.4,
                        }}
                      >
                        {a.severity}
                      </span>
                    </div>
                    <div style={{ fontSize: "0.75rem", color: "#666", fontFamily: "monospace", marginTop: 2 }}>
                      {a.measure} · {a.granularity} · {a.period_start.slice(0, 10)}
                    </div>
                    <div
                      style={{
                        display: "flex",
                        gap: "1.5rem",
                        marginTop: "0.5rem",
                        fontSize: "0.85rem",
                        fontFamily: "monospace",
                      }}
                    >
                      <span>
                        observed <strong>{formatNumber(a.observed)}</strong>
                      </span>
                      <span style={{ color: "#666" }}>
                        expected {formatNumber(a.expected)}
                      </span>
                      <span style={{ color: deviation >= 0 ? "#10b981" : "#ef4444" }}>
                        {deviation >= 0 ? "+" : ""}
                        {formatNumber(deviation)} · z={a.z_score.toFixed(2)}
                      </span>
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: "0.4rem", alignItems: "flex-start" }}>
                    {a.status !== "acknowledged" && (
                      <button
                        onClick={() => updateStatus(a.id, "acknowledged")}
                        disabled={busyId === a.id}
                        className="btn btn-secondary"
                        style={{ fontSize: "0.75rem", padding: "0.3rem 0.6rem" }}
                      >
                        Ack
                      </button>
                    )}
                    {a.status !== "dismissed" && (
                      <button
                        onClick={() => updateStatus(a.id, "dismissed")}
                        disabled={busyId === a.id}
                        className="btn btn-secondary"
                        style={{ fontSize: "0.75rem", padding: "0.3rem 0.6rem" }}
                      >
                        Dismiss
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
