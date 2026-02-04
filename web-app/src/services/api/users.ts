import type { UserInfo, UserListResponse } from "@/types/auth";
import { apiClient } from "./axios";

export class UserService {
  static async getUsers(workspaceId: string): Promise<UserListResponse> {
    const response = await apiClient.get(`/workspaces/${workspaceId}/users`);
    return response.data;
  }

  static async getAllUsers(): Promise<UserListResponse> {
    const response = await apiClient.get("/users");
    return response.data;
  }

  static async batchGetUsers(userIds: string[]): Promise<UserListResponse> {
    const response = await apiClient.post("/users/batch", {
      user_ids: userIds
    });
    return response.data;
  }

  static async getCurrentUser(): Promise<UserInfo> {
    const response = await apiClient.get("/user");
    return response.data;
  }

  static async updateUserRole(workspaceId: string, userId: string, role: string): Promise<void> {
    const response = await apiClient.put(`/workspaces/${workspaceId}/users/${userId}`, {
      role,
      user_id: userId
    });
    return response.data;
  }

  static async addUserToWorkspace(workspaceId: string, email: string, role: string): Promise<void> {
    const response = await apiClient.post(`/workspaces/${workspaceId}/users`, {
      role,
      email
    });
    return response.data;
  }

  static async updateUser(
    userId: string,
    updates: { status?: string; role?: string }
  ): Promise<void> {
    const response = await apiClient.put(`/users/${userId}`, updates);
    return response.data;
  }

  static async removeUser(workspaceId: string, userId: string): Promise<void> {
    const response = await apiClient.delete(`/workspaces/${workspaceId}/users/${userId}`);
    return response.data;
  }
}
