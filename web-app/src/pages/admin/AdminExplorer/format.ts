/** Relative "5m ago" / date for the explorer tables. */
export function ago(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const s = Math.floor((Date.now() - d.getTime()) / 1000);
  if (s < 0) return "just now";
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const days = Math.floor(h / 24);
  if (days < 30) return `${days}d ago`;
  return d.toLocaleDateString();
}

/** Tenant label for a row: "Workspace · Org", falling back gracefully. */
export function tenantLabel(workspaceName: string | null, orgName: string | null): string {
  if (workspaceName && orgName) return `${workspaceName} · ${orgName}`;
  return workspaceName ?? orgName ?? "—";
}
