import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import type { InboxItem } from "@/types/notifications";
import { refetchCatalog, updateCatalogUnread, withItemRead } from "./catalogUnread";
import { invalidateInbox } from "./invalidateInbox";

/**
 * Mark one notification read. The catalogue's counts drop by one at once; the
 * lists and the badge are re-read after. A failure re-reads the catalogue too.
 */
export const useMarkNotificationRead = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (item: InboxItem) => NotificationsService.markRead(item.id),
    onMutate: (item) => updateCatalogUnread(qc, (catalog) => withItemRead(catalog, item)),
    onError: () => {
      refetchCatalog(qc);
      toast.error("Couldn't mark that notification read");
    },
    onSettled: () => invalidateInbox(qc)
  });
};
