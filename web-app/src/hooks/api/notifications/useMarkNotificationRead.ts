import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** Mark one notification read. The list and the badge are both re-read after. */
export const useMarkNotificationRead = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => NotificationsService.markRead(id),
    onError: () => {
      toast.error("Couldn't mark that notification read");
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.notifications.inbox() });
      qc.invalidateQueries({ queryKey: queryKeys.notifications.unread() });
    }
  });
};
