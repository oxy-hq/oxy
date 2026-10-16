import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { subscribeThisDevice } from "@/libs/webPush";
import { NotificationsService } from "@/services/api/notifications";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

/**
 * Subscribe this browser to push and register it. Takes the deployment's VAPID
 * public key. The device count in the catalogue is re-read after.
 */
export const useEnablePushOnThisDevice = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (publicKey: string) => {
      const endpoint = await subscribeThisDevice(publicKey);
      await NotificationsService.registerDevice({ platform: "web", token: endpoint });
    },
    onSuccess: () => {
      toast.success("Push is on for this device");
    },
    onError: (err) => {
      toast.error(errMessage(err, "Couldn't turn on push"));
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.notifications.catalog() });
    }
  });
};
