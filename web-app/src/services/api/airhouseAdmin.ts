import { apiClient } from "./axios";

/** One workspace on the Airhouse fleet list, with its tenant if it has one. */
export interface AirhouseFleetRow {
  workspace_id: string;
  workspace_name: string;
  org_id: string | null;
  org_name: string;
  /** `none` when the workspace has no tenant yet. */
  status: string;
  tenant_id: string;
  bucket: string;
  prefix: string;
  /**
   * Whether a service account is bound AND its bearer is sealed. Without both,
   * the workspace cannot mint the ephemeral credentials every query uses — it
   * is provisioned in name only, which is invisible until someone runs a query.
   */
  service_account_ready: boolean;
  sa_rotated_at: string | null;
}

/** The fleet, and whether the server had to cut it short. */
export interface AirhouseFleet {
  rows: AirhouseFleetRow[];
  /**
   * Which halves are incomplete, by cause. The read is capped and ordered in
   * SQL, so a truncated page is a defined prefix rather than an arbitrary
   * subset — but it is still a prefix, and the two causes need different words:
   * normally only the "no warehouse" half is cut, and saying "every
   * provisioned workspace is shown" is then true. When `provisioned` is set it
   * is not, and that sentence would assert the opposite of what happened.
   */
  truncated: FleetTruncation;
}

export interface FleetTruncation {
  /** Workspaces without a warehouse were cut. The provisioned half is whole. */
  unprovisioned: boolean;
  /** The provisioned half hit its own cap, so a warehouse may be missing. */
  provisioned: boolean;
}

export async function listAirhouseFleet(): Promise<AirhouseFleet> {
  const response = await apiClient.get("/admin/airhouse");
  const data = response.data as AirhouseFleet | undefined;
  if (!data || !Array.isArray(data.rows)) {
    throw new Error(
      "GET /admin/airhouse did not return a fleet. The server is probably running a " +
        "build without this route — restart it."
    );
  }
  return data;
}

export async function provisionAirhouseTenant(workspaceId: string): Promise<AirhouseFleetRow> {
  const response = await apiClient.post(`/admin/workspaces/${workspaceId}/airhouse/provision`);
  return response.data as AirhouseFleetRow;
}
