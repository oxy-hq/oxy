import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { TestFileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useTestFile(pathb64: string, enabled = true) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.testFile.get(pathb64, project.id, branchName),
    queryFn: () => TestFileService.getTestFile(project.id, branchName, pathb64),
    enabled
  });
}
