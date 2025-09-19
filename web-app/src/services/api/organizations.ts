import { apiClient } from "./axios";
import {
  Organization,
  OrganizationListResponse,
  CreateOrganizationRequest,
} from "@/types/organization";

export class OrganizationService {
  static async listOrganizations(): Promise<OrganizationListResponse> {
    const response = await apiClient.get("/organizations");
    return response.data;
  }

  static async createOrganization(
    request: CreateOrganizationRequest
  ): Promise<Organization> {
    const response = await apiClient.post("/organizations", request);
    return response.data;
  }
}
