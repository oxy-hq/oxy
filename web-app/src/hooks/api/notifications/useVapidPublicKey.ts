import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** The deployment's Web Push key, or `configured: false` when push is not set up. */
export const useVapidPublicKey = () =>
  useQuery({
    queryKey: queryKeys.notifications.vapidPublicKey(),
    queryFn: () => NotificationsService.vapidPublicKey(),
    // Changes only with a redeploy.
    staleTime: Number.POSITIVE_INFINITY
  });
