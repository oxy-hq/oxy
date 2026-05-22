import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { getPreaggStatus } from "@/services/api/semantic";
import queryKeys from "./queryKey";

export default function usePreaggStatus() {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.preagg.status(project.id, branchName),
    queryFn: () => getPreaggStatus(project.id, branchName),
    staleTime: 30_000,
    retry: false
  });
}
