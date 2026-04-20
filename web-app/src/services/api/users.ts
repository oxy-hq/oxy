import type { UserInfo } from "@/types/auth";
import { apiClient } from "./axios";

export const UserService = {
  async getCurrentUser(): Promise<UserInfo> {
    const response = await apiClient.get("/user");
    return response.data;
  }
};
