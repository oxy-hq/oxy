import { apiClient } from "./axios";
import { UserListResponse, UserInfo } from "@/types/auth";

export class UserService {
  static async getUsers(organizationId: string): Promise<UserListResponse> {
    const response = await apiClient.get(
      `/organizations/${organizationId}/users`,
    );
    return response.data;
  }

  static async getCurrentUser(): Promise<UserInfo> {
    const response = await apiClient.get("/user");
    return response.data;
  }

  static async updateUserRole(
    organizationId: string,
    userId: string,
    role: string,
  ): Promise<void> {
    const response = await apiClient.put(
      `/organizations/${organizationId}/users/${userId}`,
      {
        role,
        user_id: userId,
      },
    );
    return response.data;
  }

  static async addUserToOrganization(
    organizationId: string,
    email: string,
    role: string,
  ): Promise<void> {
    const response = await apiClient.post(
      `/organizations/${organizationId}/users`,
      {
        role,
        email,
      },
    );
    return response.data;
  }

  static async updateUser(
    userId: string,
    updates: { status?: string; role?: string },
  ): Promise<void> {
    const response = await apiClient.put(`/users/${userId}`, updates);
    return response.data;
  }

  static async removeUser(
    organizationId: string,
    userId: string,
  ): Promise<void> {
    const response = await apiClient.delete(
      `/organizations/${organizationId}/users/${userId}`,
    );
    return response.data;
  }
}
