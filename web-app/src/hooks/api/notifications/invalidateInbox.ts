import type { QueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";

/**
 * Re-read what shows read state cheaply: the bell's list and the page's (one
 * prefix), and the badge. Not the catalogue — a read updates its counts in the
 * cache (`catalogUnread`), and it refreshes on its own staleness and on focus.
 */
export function invalidateInbox(qc: QueryClient) {
  qc.invalidateQueries({ queryKey: queryKeys.notifications.inbox() });
  qc.invalidateQueries({ queryKey: queryKeys.notifications.unread() });
}
