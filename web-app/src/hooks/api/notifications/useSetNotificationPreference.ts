import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { NotificationsService } from "@/services/api/notifications";
import type {
  CategoryNode,
  NotificationCatalog,
  NotificationChannel,
  SetNotificationPreference
} from "@/types/notifications";
import queryKeys from "../queryKey";

function toggled(
  muted: NotificationChannel[],
  { channel, muted: on }: SetNotificationPreference
): NotificationChannel[] {
  const others = muted.filter((c) => c !== channel);
  return on ? [...others, channel] : others;
}

function inCategories(
  categories: CategoryNode[],
  input: SetNotificationPreference
): CategoryNode[] {
  return categories.map((node) => {
    const own =
      "scope" in input && input.scope === node.path
        ? { ...node, muted: toggled(node.muted, input) }
        : node;
    return {
      ...own,
      kinds:
        "kind" in input
          ? own.kinds.map((k) =>
              k.kind === input.kind ? { ...k, muted: toggled(k.muted, input) } : k
            )
          : own.kinds,
      categories: inCategories(own.categories, input)
    };
  });
}

/**
 * `catalog` with one channel muted or unmuted at one node: a whole source
 * (`scope: ""`), a category (`scope: "<path>"`), or a kind. The source is an
 * app by id, or Oxygen when `app_id` is `null`. Everything else is untouched.
 */
export function withPreference(
  catalog: NotificationCatalog,
  input: SetNotificationPreference
): NotificationCatalog {
  return {
    ...catalog,
    sources: catalog.sources.map((source) => {
      if ((source.app?.id ?? null) !== input.app_id) return source;
      if ("scope" in input && input.scope === "") {
        return { ...source, muted: toggled(source.muted, input) };
      }
      return { ...source, categories: inCategories(source.categories, input) };
    })
  };
}

/**
 * Mute or unmute one channel at one node of the catalogue. Optimistic, so the
 * switch — and every switch it covers — moves when it is pressed, and is put
 * back if the server refuses.
 */
export const useSetNotificationPreference = () => {
  const qc = useQueryClient();
  const key = queryKeys.notifications.catalog();
  return useMutation({
    mutationFn: (input: SetNotificationPreference) => NotificationsService.setPreference(input),
    onMutate: async (input) => {
      await qc.cancelQueries({ queryKey: key });
      const previous = qc.getQueryData<NotificationCatalog>(key);
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
