import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { forgetThisDevice } from "@/libs/webPush";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationDevice } from "@/types/notifications";
import queryKeys from "../queryKey";

/**
 * Stop push to a device for good. Optimistic: the row goes at once, and comes
 * back in its place if the server refuses. Turning push on there again
 * registers it anew.
 */
export const useRemoveNotificationDevice = () => {
  const qc = useQueryClient();
  const key = queryKeys.notifications.devices();
  return useMutation({
    mutationFn: (id: string) => NotificationsService.removeDevice(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: key });
      const previous = qc.getQueryData<NotificationDevice[]>(key);
      const index = previous?.findIndex((d) => d.id === id) ?? -1;
      const removed = index >= 0 ? previous?.[index] : undefined;
      qc.setQueryData<NotificationDevice[]>(key, (old) => old?.filter((d) => d.id !== id));
      return { removed, index };
    },
    onSuccess: (_data, id) => {
      forgetThisDevice(id);
    },
    onError: (_err, _id, context) => {
      const removed = context?.removed;
      if (removed) {
        qc.setQueryData<NotificationDevice[]>(key, (old = []) => [
          ...old.slice(0, context.index),
          removed,
          ...old.slice(context.index)
        ]);
      }
      toast.error("Couldn't remove that device");
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: key });
    }
  });
};
