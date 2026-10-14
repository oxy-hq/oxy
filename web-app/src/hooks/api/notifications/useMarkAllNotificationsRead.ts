import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** Mark everything in the inbox read. The list and the badge are both re-read after. */
export const useMarkAllNotificationsRead = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => NotificationsService.markAllRead(),
    onError: () => {
      toast.error("Couldn't mark notifications read");
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.notifications.inbox() });
      qc.invalidateQueries({ queryKey: queryKeys.notifications.unread() });
    }
  });
};
