import { apiClient } from "./axios";
import { UserListResponse, UserInfo } from "@/types/auth";

export class UserService {
  static async getUsers(): Promise<UserListResponse> {
    const response = await apiClient.get("/users");
    return response.data;
  }

  static async getCurrentUser(): Promise<UserInfo> {
    const response = await apiClient.get("/me");
    return response.data;
  }

  static async updateUserRole(userId: string, role: string): Promise<void> {
    const response = await apiClient.put(`/users/${userId}/role`, { role });
    return response.data;
  }

  static async updateUser(
    userId: string,
    updates: { status?: string; role?: string },
  ): Promise<void> {
    const response = await apiClient.put(`/users/${userId}`, updates);
    return response.data;
  }

  static async deleteUser(userId: string): Promise<void> {
    const response = await apiClient.delete(`/users/${userId}`);
    return response.data;
  }
}
