import { useQuery } from "@tanstack/react-query";
import { NotificationsService } from "@/services/api/notifications";
import queryKeys from "../queryKey";

/** One notification of mine, by id — what a push that carried only the id opens. */
export const useNotification = (id: string | undefined) =>
  useQuery({
    queryKey: queryKeys.notifications.item(id ?? ""),
    queryFn: () => NotificationsService.item(id ?? ""),
    enabled: !!id
  });
