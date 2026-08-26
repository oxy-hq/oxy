import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CustomAppsService } from "@/services/api/customApps";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

/**
 * Published custom apps for a workspace. Drives the HQ launcher and
 * workspace rail. Empty array (not error) when the workspace has none —
 * callers can render conditionally.
 */
export const useCustomApps = (workspaceId: string) =>
  useQuery({
    queryKey: queryKeys.customApps.list(workspaceId),
    queryFn: () => CustomAppsService.listForWorkspace(workspaceId),
    enabled: !!workspaceId
  });

export const usePublishApp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: CustomAppsService.publish,
    onSuccess: (app) => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
      // Workspace-scoped list — invalidate the one for this app's workspace
      // so the sidebar picks up the new entry immediately.
      qc.invalidateQueries({ queryKey: queryKeys.customApps.list(app.project_id) });
      toast.success(`${app.name} published`);
    },
    onError: (err) => toast.error(errMessage(err, "Failed to publish"))
  });
};

export const useUnpublishApp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: CustomAppsService.unpublish,
    onSuccess: (app) => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
      qc.invalidateQueries({ queryKey: queryKeys.customApps.list(app.project_id) });
      toast.success(`${app.name} unpublished`);
    },
    onError: (err) => toast.error(errMessage(err, "Failed to unpublish"))
  });
};

/**
 * Paged admin list of custom apps, ordered by `updated_at` DESC so
 * the first page is the recently-active set — what staff usually
 * want when they open the admin. Additional pages walk back in time
 * via the server-returned `next_offset`; we only fetch more when the
 * user explicitly asks (no infinite-scroll auto-prefetch).
 */
export const useAdminApps = (pageSize = 50) =>
  useInfiniteQuery({
    queryKey: [...queryKeys.customApps.all(), { pageSize }],
    queryFn: ({ pageParam }) =>
      CustomAppsService.list({ limit: pageSize, offset: pageParam as number }),
    initialPageParam: 0,
    getNextPageParam: (last) => last.next_offset
  });

/**
 * Diagnostic snapshot. Only fires when both slugs are present so the
 * hook is cheap to mount even on the list view. The 30s staleTime
 * keeps tab switches snappy without hiding fresh edits for long.
 */
export const useAppDebug = (orgSlug: string | undefined, appSlug: string | undefined) =>
  useQuery({
    queryKey: queryKeys.customApps.debug(orgSlug ?? "", appSlug ?? ""),
    queryFn: () =>
      CustomAppsService.debug({
        org_slug: orgSlug as string,
        slug: appSlug as string
      }),
    enabled: !!orgSlug && !!appSlug,
    staleTime: 30_000
  });

/**
 * List the curated scaffold templates. Templates are baked into the
 * server binary and never change at runtime, so we cache them forever.
 */
export function useListTemplates() {
  return useQuery({
    queryKey: queryKeys.customApps.templates(),
    queryFn: () => CustomAppsService.listTemplates(),
    // Templates are baked into the binary — never change at runtime.
    staleTime: Infinity
  });
}

/**
 * PATCH /customer-apps/{id}. Used by the Settings tab to fix things
 * like a wrong LocalFolder path without having to delete + recreate.
 *
 * Server-side validation hits configurable surfaces (e.g. "the
 * configured local path doesn't have an index.html"). Those come
 * back as `app.warnings[]` — we surface them as separate warning
 * toasts after the success toast so the operator catches them
 * without having to dig.
 */
export const useUpdateApp = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; req: import("@/types/apps").UpdateAppRequest }) =>
      CustomAppsService.update(args.id, args.req),
    onSuccess: (app) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.customApps.all() });
      toast.success("App updated");
      for (const warning of app.warnings ?? []) {
        toast.warning(warning, { duration: 8000 });
      }
    },
    onError: (err) => toast.error(errMessage(err, "Update failed"))
  });
};

// ── Activity (usage tracking) ───────────────────────────────────────────

/**
 * Headline numbers (last viewed, 7d uniques, 7d total views/events)
 * for the AppDetail Activity tab. 30s staleTime so navigating away
 * and back doesn't refetch; the data isn't real-time-critical.
 */
export const useAppActivitySummary = (appId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.customApps.activitySummary(appId ?? ""),
    queryFn: () => CustomAppsService.activitySummary(appId as string),
    enabled: !!appId,
    staleTime: 30_000
  });

export const useAppActivityVisitors = (appId: string | undefined, days = 7) =>
  useQuery({
    queryKey: queryKeys.customApps.activityVisitors(appId ?? "", days),
    queryFn: () => CustomAppsService.activityVisitors(appId as string, days),
    enabled: !!appId,
    staleTime: 30_000
  });

/**
 * Per-event-name counts + last-fired timestamps for the Activity tab's
 * Events table. Split from the occurrences hook because they return
 * different row shapes — folding both into one `useQuery` widened
 * the `queryFn` return into a union TanStack Query's overloads
 * can't satisfy (TS2769 in CI). One hook per shape keeps the
 * generics clean.
 */
export const useAppActivityEventGroups = (appId: string | undefined, days = 7) =>
  useQuery({
    queryKey: queryKeys.customApps.activityEvents(appId ?? "", days, null),
    queryFn: () => CustomAppsService.activityEventGroups(appId as string, days),
    enabled: !!appId,
    staleTime: 30_000
  });

/**
 * Recent occurrences for a single event_name — the drill-down view
 * the Activity tab opens when an operator clicks a row in the
 * Events table. Disabled when `eventName` is null so the call site
 * can `useState<string | null>` and pass it through without a
 * conditional hook.
 */
export const useAppActivityEventOccurrences = (
  appId: string | undefined,
  eventName: string | null,
  days = 7
) =>
  useQuery({
    queryKey: queryKeys.customApps.activityEvents(appId ?? "", days, eventName),
    queryFn: () =>
      CustomAppsService.activityEventOccurrences(appId as string, eventName as string, days),
    enabled: !!appId && !!eventName,
    staleTime: 30_000
  });
