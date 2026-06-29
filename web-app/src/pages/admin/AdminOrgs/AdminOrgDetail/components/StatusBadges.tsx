import type { WorkspaceStatusId } from "@/services/api/adminTenants";
import { AdminStatusPill } from "../../../components/AdminStatusPill";

const ROLE_TONE: Record<string, "ok" | "info" | "muted"> = {
  owner: "ok",
  admin: "info",
  member: "muted"
};

export const RoleBadge = ({ role }: { role: string }) => (
  <AdminStatusPill tone={ROLE_TONE[role.toLowerCase()] ?? "muted"} label={role} />
);

const WORKSPACE_STATUS: Record<
  string,
  { tone: "ok" | "warn" | "danger" | "muted"; label: string }
> = {
  ready: { tone: "ok", label: "Ready" },
  cloning: { tone: "warn", label: "Cloning" },
  failed: { tone: "danger", label: "Failed" },
  not_oxy_project: { tone: "muted", label: "Not Oxy" }
};

export const WorkspaceStatusPill = ({ status }: { status: WorkspaceStatusId | string }) => {
  const v = WORKSPACE_STATUS[status] ?? { tone: "muted" as const, label: status };
  return <AdminStatusPill tone={v.tone} label={v.label} />;
};
