import { useMutation, useQueryClient } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationDevice, OsPermission } from "@/types/notifications";
import queryKeys from "../queryKey";

/**
 * Tell the server what this browser allows, the way the mobile app reports
 * its OS state on launch: it is what turns a web device's row into "Blocked
 * in browser settings". Quiet on failure — nobody asked for it — but logged.
 */
export const useReportThisDevice = () => {
  const qc = useQueryClient();
  const key = queryKeys.notifications.devices();
  return useMutation({
    mutationFn: ({ id, permission }: { id: string; permission: OsPermission }) =>
      NotificationsService.updateDevice(id, { os_permission: permission }),
    onSuccess: (_data, { id, permission }) => {
      qc.setQueryData<NotificationDevice[]>(key, (old) =>
        old?.map((d) => (d.id === id ? { ...d, os_permission: permission } : d))
      );
    },
    onError: (err) => {
      console.error("Couldn't report this browser's notification permission", err);
    }
  });
};
