import { useEffect } from "react";
import { useReportThisDevice } from "@/hooks/api/notifications";
import { browserPermission } from "@/libs/webPush";
import type { NotificationDevice } from "@/types/notifications";

/**
 * Keep this browser's row in step with what the browser allows. The mobile
 * app reports its OS state on launch; for a browser, opening Settings is the
 * moment. Sends nothing when the row already says the same.
 */
export function useReportBrowserPermission(device: NotificationDevice | undefined) {
  const { mutate } = useReportThisDevice();
  const id = device?.id;
  const reported = device?.os_permission;
  useEffect(() => {
    if (!id) return;
    const permission = browserPermission();
    if (permission && permission !== reported) mutate({ id, permission });
  }, [id, reported, mutate]);
}
