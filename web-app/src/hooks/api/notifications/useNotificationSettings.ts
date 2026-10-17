import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/**
 * Everything the signed-in person has set about notifications — pause, quiet
 * hours, and every node of every source's tree — as one versioned document.
 * Settings draws from it; the bell and the notifications page keep reading
 * the catalogue.
 */
export const useNotificationSettings = () =>
  useQuery({
    queryKey: queryKeys.notifications.settings(),
    queryFn: () => NotificationsService.settings()
  });
