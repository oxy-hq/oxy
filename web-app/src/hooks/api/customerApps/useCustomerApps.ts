import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CustomerAppsService } from "@/services/api/customerApps";
import queryKeys from "../queryKey";

/**
 * Paged admin list of customer apps, ordered by `updated_at` DESC so
 * the first page is the recently-active set — what staff usually
 * want when they open the admin. Additional pages walk back in time
 * via the server-returned `next_offset`; we only fetch more when the
 * user explicitly asks (no infinite-scroll auto-prefetch).
 */
export const useAdminApps = (pageSize = 50) =>
  useInfiniteQuery({
    queryKey: [...queryKeys.customerApps.all(), { pageSize }],
    queryFn: ({ pageParam }) =>
      CustomerAppsService.list({ limit: pageSize, offset: pageParam as number }),
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
    queryKey: queryKeys.customerApps.debug(orgSlug ?? "", appSlug ?? ""),
    queryFn: () =>
      CustomerAppsService.debug({
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
    queryKey: queryKeys.customerApps.templates(),
    queryFn: () => CustomerAppsService.listTemplates(),
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
      CustomerAppsService.update(args.id, args.req),
    onSuccess: (app) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.customerApps.all() });
      toast.success("App updated");
      for (const warning of app.warnings ?? []) {
        toast.warning(warning, { duration: 8000 });
      }
    },
    onError: (err) => {
      const message = err instanceof Error ? err.message : "Update failed";
      toast.error(message);
    }
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
    queryKey: queryKeys.customerApps.activitySummary(appId ?? ""),
    queryFn: () => CustomerAppsService.activitySummary(appId as string),
    enabled: !!appId,
    staleTime: 30_000
  });

export const useAppActivityVisitors = (appId: string | undefined, days = 7) =>
  useQuery({
    queryKey: queryKeys.customerApps.activityVisitors(appId ?? "", days),
    queryFn: () => CustomerAppsService.activityVisitors(appId as string, days),
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
    queryKey: queryKeys.customerApps.activityEvents(appId ?? "", days, null),
    queryFn: () => CustomerAppsService.activityEventGroups(appId as string, days),
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
    queryKey: queryKeys.customerApps.activityEvents(appId ?? "", days, eventName),
    queryFn: () =>
      CustomerAppsService.activityEventOccurrences(appId as string, eventName as string, days),
    enabled: !!appId && !!eventName,
    staleTime: 30_000
  });
