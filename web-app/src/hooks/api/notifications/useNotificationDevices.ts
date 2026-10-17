import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** Every browser and phone push reaches for the signed-in person. */
export const useNotificationDevices = () =>
  useQuery({
    queryKey: queryKeys.notifications.devices(),
    queryFn: () => NotificationsService.devices()
  });
