import type {
  AssignmentRow,
  AssignmentsFilter,
  CreateAssignmentRequest,
  CreateLocationRequest,
  CreateRoleRequest,
  ExternalIdRow,
  ListAssignmentsResponse,
  ListLocationsResponse,
  LocationRow,
  RoleRow,
  UpdateLocationRequest
} from "@/types/operatingGraph";
import { apiClient } from "./axios";

/**
 * The operating graph, org-admin side: locations, the position vocabulary
 * (`org_roles`) and assignments. Reads take any org member; every write needs
 * an org admin. Errors answer `{ error }`.
 */
export class OperatingGraphService {
  // ── Locations ──

  /** Every location of the org, with `external_ids` batched in. */
  static async listLocations(orgId: string): Promise<ListLocationsResponse> {
    const response = await apiClient.get<ListLocationsResponse>(`/orgs/${orgId}/locations`);
    return response.data;
  }

  static async createLocation(orgId: string, request: CreateLocationRequest): Promise<LocationRow> {
    const response = await apiClient.post<LocationRow>(`/orgs/${orgId}/locations`, request);
    return response.data;
  }

  /** 400 when the new parent would make the location its own ancestor. */
  static async updateLocation(
    orgId: string,
    locationId: string,
    request: UpdateLocationRequest
  ): Promise<LocationRow> {
    const response = await apiClient.patch<LocationRow>(
      `/orgs/${orgId}/locations/${locationId}`,
      request
    );
    return response.data;
  }

  /** 409 when another location of the org already carries that id in that system. */
  static async setExternalId(
    orgId: string,
    locationId: string,
    system: string,
    externalId: string
  ): Promise<ExternalIdRow> {
    const response = await apiClient.put<ExternalIdRow>(
      `/orgs/${orgId}/locations/${locationId}/external-ids/${encodeURIComponent(system)}`,
      { external_id: externalId }
    );
    return response.data;
  }

  static async deleteExternalId(orgId: string, locationId: string, system: string): Promise<void> {
    await apiClient.delete(
      `/orgs/${orgId}/locations/${locationId}/external-ids/${encodeURIComponent(system)}`
    );
  }

  // ── Positions (org roles) ──

  /** A bare array, unlike the other lists — this route predates them. */
  static async listRoles(orgId: string): Promise<RoleRow[]> {
    const response = await apiClient.get<RoleRow[]>(`/orgs/${orgId}/roles`);
    return response.data;
  }

  static async createRole(orgId: string, request: CreateRoleRequest): Promise<RoleRow> {
    const response = await apiClient.post<RoleRow>(`/orgs/${orgId}/roles`, request);
    return response.data;
  }

  static async renameRole(orgId: string, roleId: string, name: string): Promise<RoleRow> {
    const response = await apiClient.patch<RoleRow>(`/orgs/${orgId}/roles/${roleId}`, { name });
    return response.data;
  }

  /** 409 while anyone holds the position. */
  static async deleteRole(orgId: string, roleId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/roles/${roleId}`);
  }

  // ── Assignments ──

  static async listAssignments(
    orgId: string,
    filter: AssignmentsFilter = {}
  ): Promise<ListAssignmentsResponse> {
    const response = await apiClient.get<ListAssignmentsResponse>(`/orgs/${orgId}/assignments`, {
      params: filter
    });
    return response.data;
  }

  /** Idempotent: 200 with the existing row when it is already held. */
  static async createAssignment(
    orgId: string,
    request: CreateAssignmentRequest
  ): Promise<AssignmentRow> {
    const response = await apiClient.post<AssignmentRow>(`/orgs/${orgId}/assignments`, request);
    return response.data;
  }

  static async deleteAssignment(orgId: string, assignmentId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/assignments/${assignmentId}`);
  }
}
