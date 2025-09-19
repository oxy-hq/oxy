export interface Organization {
  id: string;
  name: string;
  role: string;
  created_at: string;
  updated_at: string;
}

export interface OrganizationListResponse {
  organizations: Organization[];
  total: number;
}

export interface CreateOrganizationRequest {
  name: string;
}

export interface MessageResponse {
  message: string;
}
