import { useQuery } from "@tanstack/react-query";
import { ProjectService } from "@/services/api";
import queryKeys from "./api/queryKey";

export const useProjectStatus = (project_id: string) => {
  return useQuery({
    queryKey: queryKeys.settings.projectStatus(project_id),
    queryFn: () => ProjectService.getProjectStatus(project_id)
  });
};
