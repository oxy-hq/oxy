import { useInfiniteQuery } from "@tanstack/react-query";
import { type RecentCommitsResponse, WorkspaceService } from "@/services/api/workspaces";
import queryKeys from "../queryKey";

const PAGE_SIZE = 30;

interface Args {
  workspaceId?: string;
  branch?: string;
  enabled?: boolean;
}

/**
 * Infinite-scroll commit history for the full History dialog.
 * Each page is `PAGE_SIZE` commits; `has_more` from the server drives the
 * next-page cursor.
 */
export function useRecentCommitsInfinite({ workspaceId, branch, enabled = true }: Args) {
  return useInfiniteQuery<
    RecentCommitsResponse,
    Error,
    { pages: RecentCommitsResponse[]; pageParams: number[] },
    ReturnType<typeof queryKeys.workspaces.recentCommits>,
    number
  >({
    queryKey: queryKeys.workspaces.recentCommits(workspaceId ?? "", branch ?? ""),
    queryFn: ({ pageParam }) =>
      WorkspaceService.getRecentCommits(workspaceId ?? "", branch ?? "", {
        limit: PAGE_SIZE,
        offset: pageParam
      }),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) =>
      lastPage.has_more ? allPages.length * PAGE_SIZE : undefined,
    enabled: enabled && !!workspaceId && !!branch,
    staleTime: 10_000
  });
}
