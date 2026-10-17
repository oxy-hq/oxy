import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  browserDeviceName,
  rememberThisDevice,
  subscribeThisDevice,
  thisDeviceId
} from "@/libs/webPush";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationDevice } from "@/types/notifications";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

/**
 * Subscribe this browser to push and register it. Takes the deployment's VAPID
 * public key. The id the server answers with is remembered, so Settings can
 * mark the row "This device"; a name the person gave that row is sent back
 * rather than replaced. Devices, settings and the catalogue are re-read after.
 */
export const useEnablePushOnThisDevice = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (publicKey: string) => {
      const endpoint = await subscribeThisDevice(publicKey);
      const known = qc
        .getQueryData<NotificationDevice[]>(queryKeys.notifications.devices())
        ?.find((d) => d.id === thisDeviceId());
      const id = await NotificationsService.registerDevice({
        platform: "web",
        token: endpoint,
        device_name: known?.name ?? browserDeviceName(navigator.userAgent)
      });
      if (id) rememberThisDevice(id);
    },
    onSuccess: () => {
      toast.success("Push is on for this device");
    },
    onError: (err) => {
      toast.error(errMessage(err, "Couldn't turn on push"));
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.notifications.devices() });
      qc.invalidateQueries({ queryKey: queryKeys.notifications.settings() });
      qc.invalidateQueries({ queryKey: queryKeys.notifications.catalog() });
    }
  });
};
