import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/**
 * Every source that can tell the signed-in person something — Oxygen and each
 * app — as a tree of categories and kinds, with mutes, unread counts, and where
 * push and email reach them. The page's tree and Settings both draw from it.
 */
export const useNotificationCatalog = () =>
  useQuery({
    queryKey: queryKeys.notifications.catalog(),
    queryFn: () => NotificationsService.catalog()
  });
