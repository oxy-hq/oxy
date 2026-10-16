import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import type { InboxFilter } from "@/types/notifications";
import { refetchCatalog, updateCatalogUnread, withFilterRead } from "./catalogUnread";
import { invalidateInbox } from "./invalidateInbox";

/**
 * Mark read everything the filter covers — the whole inbox for `{}`. The
 * catalogue's counts for what it covers go to zero at once; the lists and the
 * badge are re-read after. A failure re-reads the catalogue too.
 */
export const useMarkAllNotificationsRead = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (filter: InboxFilter) => NotificationsService.markAllRead(filter),
    onMutate: (filter) => updateCatalogUnread(qc, (catalog) => withFilterRead(catalog, filter)),
    onError: () => {
      refetchCatalog(qc);
      toast.error("Couldn't mark notifications read");
    },
    onSettled: () => invalidateInbox(qc)
  });
};
