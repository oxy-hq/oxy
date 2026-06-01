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

export const useMyApps = () =>
  useQuery({
    queryKey: queryKeys.customerApps.mine(),
    queryFn: CustomerAppsService.listMine
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
      queryClient.invalidateQueries({ queryKey: queryKeys.customerApps.mine() });
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
