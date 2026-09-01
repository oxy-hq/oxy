import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AirwayService, type SpApiMarketplace } from "@/services/api/airway";
import queryKeys from "../queryKey";

/** Marketplaces the `sp_api` connector can reach.
 *
 * Fetched rather than hardcoded so the picker has one source of truth with
 * `source_factory::NA_MARKETPLACES`. `staleTime: Infinity` because the list is
 * a compile-time constant on the server — it can only change on deploy, so
 * refetching it is pure noise.
 */
export default function useSpApiMarketplaces(enabled = true) {
  const { project } = useCurrentProjectBranch();
  return useQuery<SpApiMarketplace[], Error>({
    queryKey: queryKeys.spApi.marketplaces(project.id),
    queryFn: () => AirwayService.listSpApiMarketplaces(project.id),
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
    refetchOnMount: false
  });
}
