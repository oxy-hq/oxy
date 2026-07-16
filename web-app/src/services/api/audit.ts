import type { AuditEvent, AuditSearchParams } from "@/types/audit";
import { apiClient } from "./axios";

/** Platform audit-log search (Oxy staff; `GET /api/admin/audit`). */
export const AuditService = {
  async search(params: AuditSearchParams): Promise<AuditEvent[]> {
    const { data } = await apiClient.get("/admin/audit", { params });
    return data;
  }
};
