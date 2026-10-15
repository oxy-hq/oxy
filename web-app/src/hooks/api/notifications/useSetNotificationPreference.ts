import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import type {
  KindPreference,
  NotificationPreferences,
  SetNotificationPreference
} from "@/types/notifications";
import queryKeys from "../queryKey";

function withMuted(
  kinds: KindPreference[],
  { kind, channel, muted }: SetNotificationPreference
): KindPreference[] {
  return kinds.map((k) => {
    if (k.kind !== kind) return k;
    const others = k.muted.filter((c) => c !== channel);
    return { ...k, muted: muted ? [...others, channel] : others };
  });
}

/**
 * `prefs` with one channel of one kind muted or unmuted — an app's kind, or an
 * Oxygen kind when `app_id` is `null`. Everything else is untouched.
 */
export function withPreference(
  prefs: NotificationPreferences,
  input: SetNotificationPreference
): NotificationPreferences {
  if (input.app_id === null) {
    return { ...prefs, platform: withMuted(prefs.platform, input) };
  }
  return {
    ...prefs,
    apps: prefs.apps.map((entry) =>
      entry.app.id === input.app_id ? { ...entry, kinds: withMuted(entry.kinds, input) } : entry
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
