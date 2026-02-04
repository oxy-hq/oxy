import { useMutation } from "@tanstack/react-query";
import { DatabaseService } from "@/services/api";
import useCurrentProjectBranch from "../useCurrentProjectBranch";

export default function useExecuteSql() {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation({
    mutationFn: (data: { pathb64: string; sql: string; database: string }) =>
      DatabaseService.executeSql(projectId, branchName, data.pathb64, data.sql, data.database)
  });
}
