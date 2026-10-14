import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** What the signed-in person has muted, per app and kind, and where push and email reach them. */
export const useNotificationPreferences = () =>
  useQuery({
    queryKey: queryKeys.notifications.preferences(),
    queryFn: () => NotificationsService.preferences()
  });
