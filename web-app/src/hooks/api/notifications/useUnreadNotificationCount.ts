import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** How often the badge re-reads its count while the tab is visible. */
export const UNREAD_POLL_MS = 60_000;

/**
 * The bell's badge count. Polled, and re-read when the window regains focus — a
 * push that arrives while the tab is open also marks it stale (see the inbox bell).
 */
export const useUnreadNotificationCount = () =>
  useQuery({
    queryKey: queryKeys.notifications.unread(),
    queryFn: () => NotificationsService.unread(),
    select: (data) => data.unread,
    refetchInterval: UNREAD_POLL_MS,
    refetchOnWindowFocus: true
  });
