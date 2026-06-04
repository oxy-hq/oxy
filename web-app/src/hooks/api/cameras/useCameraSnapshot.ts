import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { apiClient } from "@/services/api/axios";
import queryKeys from "../queryKey";

/**
 * Polls the snapshot proxy and exposes an `<img>`-friendly object URL.
 *
 * **Why a separate hook (not a plain `useQuery`):** each refetch
 * returns a fresh Blob, which we must `URL.createObjectURL()` and
 * later `URL.revokeObjectURL()`. Without cleanup the page leaks one
 * URL per poll. We split the concerns: TanStack Query manages the
 * polling lifecycle + retries; a `useEffect` translates each new
 * Blob into a fresh object URL and revokes the previous one in its
 * cleanup.
 *
 * **Polling cadence and gating:** default interval is 15s — enough
 * "feels current" for a fleet dashboard without hammering Oxy + the
 * edge box. The `enabled` flag lets callers pause polling (e.g. the
 * thumbnail isn't visible in the viewport). When `enabled` flips
 * back to true the next refetch happens immediately, so visibility
 * scrolling feels responsive.
 *
 * Errors are intentionally collapsed to a single `isError` flag —
 * the thumbnail UI only distinguishes "have a recent frame" vs. "no
 * recent frame"; the specific 404 / 503 / 502 doesn't change
 * rendering.
 */
const useCameraSnapshot = (
  workspaceId: string | undefined,
  cameraId: string | undefined,
  intervalMs = 15000,
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const query = useQuery<Blob, Error>({
    queryKey: [...queryKeys.camera.camera(effectiveWorkspaceId, cameraId ?? ""), "snapshot"],
    queryFn: async () => {
      const resp = await apiClient.get(
        `/${effectiveWorkspaceId}/cameras/${cameraId}/preview/snapshot.jpg`,
        { responseType: "blob" }
      );
      return resp.data as Blob;
    },
    enabled: !!cameraId && enabled,
    refetchInterval: enabled ? intervalMs : false,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: false,
    staleTime: 0,
    retry: false
  });

  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!query.data) return;
    const url = URL.createObjectURL(query.data);
    setBlobUrl(url);
    return () => {
      URL.revokeObjectURL(url);
    };
  }, [query.data]);

  return {
    blobUrl,
    isLoading: query.isLoading && !blobUrl,
    isError: query.isError && !blobUrl
  };
};

export default useCameraSnapshot;
