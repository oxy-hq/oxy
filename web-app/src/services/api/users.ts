import type { UserInfo, UserListResponse } from "@/types/auth";
import { apiClient } from "./axios";

export const UserService = {
  async getAllUsers(): Promise<UserListResponse> {
    const response = await apiClient.get("/users");
    return response.data;
  },

  async batchGetUsers(userIds: string[]): Promise<UserListResponse> {
    const response = await apiClient.post("/users/batch", { user_ids: userIds });
    return response.data;
  },

  async getCurrentUser(): Promise<UserInfo> {
    const response = await apiClient.get("/user");
    return response.data;
  },

  async updateUser(userId: string, updates: { status?: string; role?: string }): Promise<void> {
    await apiClient.put(`/users/${userId}`, updates);
  },

  async deleteUser(userId: string): Promise<void> {
    await apiClient.delete(`/users/${userId}`);
  },

  async inviteUser(email: string): Promise<void> {
    await apiClient.post("/auth/invite", { email });
  }
};
