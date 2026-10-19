import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import {
  browserDeviceName,
  rememberThisDevice,
  requestPushPermission,
  subscribeThisDevice,
  thisDeviceId
} from "@/libs/webPush";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationDevice } from "@/types/notifications";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

interface EnablePushInput {
  /** The deployment's VAPID public key. */
  publicKey: string;
  /** `Notification.requestPermission()`, already asked inside the click. */
  permission: Promise<NotificationPermission>;
}

/**
 * Subscribe this browser to push and register it, with the keys a payload is
 * encrypted to. The id the server answers with is remembered, so Settings can
 * mark the row "This device"; a name the person gave that row is sent back
 * rather than replaced. Devices, settings and the catalogue are re-read after.
 *
 * Call `turnOn(publicKey)` from the click handler: it asks for permission before
 * anything is awaited, which is what keeps the request inside the user gesture
 * (`mutate` reaches its `mutationFn` only after awaits of its own).
 */
export const useEnablePushOnThisDevice = () => {
  const qc = useQueryClient();
  const mutation = useMutation({
    mutationFn: async ({ publicKey, permission }: EnablePushInput) => {
      const { endpoint, keys } = await subscribeThisDevice(publicKey, permission);
      const known = qc
        .getQueryData<NotificationDevice[]>(queryKeys.notifications.devices())
        ?.find((d) => d.id === thisDeviceId());
      const id = await NotificationsService.registerDevice({
        platform: "web",
        token: endpoint,
        device_name: known?.name ?? browserDeviceName(navigator.userAgent),
        keys
      });
      if (id) rememberThisDevice(id);
    },
    onSuccess: () => {
      toast.success("Notifications are on for this device");
    },
    onError: (err) => {
      toast.error(errMessage(err, "Couldn't turn on notifications"));
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.notifications.devices() });
      qc.invalidateQueries({ queryKey: queryKeys.notifications.settings() });
      qc.invalidateQueries({ queryKey: queryKeys.notifications.catalog() });
    }
  });
  const { mutate } = mutation;
  const turnOn = useCallback(
    (publicKey: string) => mutate({ publicKey, permission: requestPushPermission() }),
    [mutate]
  );
  return { ...mutation, turnOn };
};
