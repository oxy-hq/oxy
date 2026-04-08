import { useQuery } from "@tanstack/react-query";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import queryKeys from "./api/queryKey";

export const useProjectStatus = (project_id: string) => {
  return useQuery({
    queryKey: queryKeys.settings.projectStatus(project_id),
    queryFn: () => ProjectService.getWorkspaceStatus(project_id)
  });
};
