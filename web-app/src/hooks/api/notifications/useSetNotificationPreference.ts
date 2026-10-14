import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import type { NotificationPreferences, SetNotificationPreference } from "@/types/notifications";
import queryKeys from "../queryKey";

/** `prefs` with one channel of one kind muted or unmuted. Everything else is untouched. */
export function withPreference(
  prefs: NotificationPreferences,
  { app_id, kind, channel, muted }: SetNotificationPreference
): NotificationPreferences {
  return {
    ...prefs,
    apps: prefs.apps.map((entry) =>
      entry.app.id !== app_id
        ? entry
        : {
            ...entry,
            kinds: entry.kinds.map((k) => {
              if (k.kind !== kind) return k;
              const others = k.muted.filter((c) => c !== channel);
              return { ...k, muted: muted ? [...others, channel] : others };
            })
          }
    )
  };
}

/**
 * Mute or unmute one channel of one kind. Optimistic, so the switch moves when it
 * is pressed, and put back if the server refuses.
 */
export const useSetNotificationPreference = () => {
  const qc = useQueryClient();
  const key = queryKeys.notifications.preferences();
  return useMutation({
    mutationFn: (input: SetNotificationPreference) => NotificationsService.setPreference(input),
    onMutate: async (input) => {
      await qc.cancelQueries({ queryKey: key });
      const previous = qc.getQueryData<NotificationPreferences>(key);
      if (previous) qc.setQueryData(key, withPreference(previous, input));
      return { previous };
    },
    onError: (_err, _input, context) => {
      if (context?.previous) qc.setQueryData(key, context.previous);
      toast.error("Couldn't save that setting");
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: key });
    }
  });
};
