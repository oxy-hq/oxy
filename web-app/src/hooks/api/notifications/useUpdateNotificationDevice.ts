import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import type { DevicePatch, NotificationDevice } from "@/types/notifications";
import queryKeys from "../queryKey";

const MUTATION_KEY = [...queryKeys.notifications.devices(), "update"] as const;

export interface DeviceUpdate {
  id: string;
  patch: DevicePatch;
}

const withDevice = (devices: NotificationDevice[] | undefined, device: NotificationDevice) =>
  devices?.map((d) => (d.id === device.id ? device : d));

/**
 * Rename a device, switch it off or on, or set its floor. Optimistic: the
 * row changes at once, and goes back to what it was if the server refuses.
 */
export const useUpdateNotificationDevice = () => {
  const qc = useQueryClient();
  const key = queryKeys.notifications.devices();
  return useMutation({
    mutationKey: MUTATION_KEY,
    mutationFn: ({ id, patch }: DeviceUpdate) => NotificationsService.updateDevice(id, patch),
    onMutate: async ({ id, patch }) => {
      await qc.cancelQueries({ queryKey: key });
      const before = qc.getQueryData<NotificationDevice[]>(key)?.find((d) => d.id === id);
      if (before) {
        qc.setQueryData<NotificationDevice[]>(key, (old) =>
          withDevice(old, { ...before, ...patch })
        );
      }
      return { before };
    },
    onError: (_err, _update, context) => {
      const before = context?.before;
      if (before) qc.setQueryData<NotificationDevice[]>(key, (old) => withDevice(old, before));
      toast.error("Couldn't update that device");
    },
    onSettled: () => {
      if (qc.isMutating({ mutationKey: MUTATION_KEY }) === 1) {
        qc.invalidateQueries({ queryKey: key });
      }
    }
  });
};
