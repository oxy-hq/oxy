import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { TestFileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useTestFiles(enabled = true) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.testFile.list(project.id, branchName),
    queryFn: () => TestFileService.listTestFiles(project.id, branchName),
    enabled
  });
}
