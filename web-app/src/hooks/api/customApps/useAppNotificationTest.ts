import { useMutation, useQuery } from "@tanstack/react-query";

import { CustomAppsService } from "@/services/api/customApps";
import queryKeys from "../queryKey";

/** The kinds an app's live build declares, and whether it can notify you. */
export function useAppNotificationKinds(id: string | undefined) {
  return useQuery({
    queryKey: queryKeys.customApps.notificationKinds(id ?? ""),
    queryFn: () => CustomAppsService.listNotificationKinds(id as string),
    enabled: !!id
  });
}

/** Send yourself one notification of a kind, as the app. Nothing to
 *  invalidate: the console lists no notifications. */
export function useSendTestNotification() {
  return useMutation({
    mutationFn: ({ id, kind }: { id: string; kind: string }) =>
      CustomAppsService.sendTestNotification(id, kind)
  });
}
