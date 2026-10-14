import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** How many rows the inbox shows. Read and unread alike, newest first. */
export const INBOX_LIMIT = 50;

/**
 * The inbox list. Mount it only while the inbox is open: it reads the store of
 * every org on the page, which the badge (`useUnreadNotificationCount`) never does.
 */
export const useNotificationInbox = () =>
  useQuery({
    queryKey: queryKeys.notifications.inbox(),
    queryFn: () => NotificationsService.inbox({ unreadOnly: false, limit: INBOX_LIMIT })
  });
