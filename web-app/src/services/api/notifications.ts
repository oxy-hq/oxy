import type {
  InboxResponse,
  NotificationPreferences,
  RegisterDeviceInput,
  SetNotificationPreference,
  UnreadResponse,
  VapidKeyResponse
} from "@/types/notifications";
import { apiClient } from "./axios";

/**
 * The signed-in person's notifications: the inbox, what they have muted, and the
 * devices push reaches. Every route is the caller's own; none takes a workspace.
 */
export class NotificationsService {
  /** Newest first. `unreadOnly` defaults to `true` server-side, so it is always sent. */
  static async inbox(params: { unreadOnly: boolean; limit: number }): Promise<InboxResponse> {
    const response = await apiClient.get<InboxResponse>("/notifications", {
      params: { unread_only: params.unreadOnly, limit: params.limit }
    });
    return response.data;
  }

  /** The badge count. Cheap: it never reads an org's store. */
  static async unread(): Promise<UnreadResponse> {
    const response = await apiClient.get<UnreadResponse>("/notifications/unread");
    return response.data;
  }

  static async markRead(id: string): Promise<void> {
    await apiClient.post(`/notifications/${encodeURIComponent(id)}/read`);
  }

  static async markAllRead(): Promise<void> {
    await apiClient.post("/notifications/read-all");
  }

  static async preferences(): Promise<NotificationPreferences> {
    const response = await apiClient.get<NotificationPreferences>("/notifications/preferences");
    return response.data;
  }

  static async setPreference(input: SetNotificationPreference): Promise<void> {
    await apiClient.put("/notifications/preferences", input);
  }

  static async vapidPublicKey(): Promise<VapidKeyResponse> {
    const response = await apiClient.get<VapidKeyResponse>("/notifications/vapid-public-key");
    return response.data;
  }

  static async registerDevice(input: RegisterDeviceInput): Promise<void> {
    await apiClient.post("/notifications/devices", input);
  }
}
