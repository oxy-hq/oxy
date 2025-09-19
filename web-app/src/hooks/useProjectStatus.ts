import { useQuery } from "@tanstack/react-query";
import queryKeys from "./api/queryKey";
import { ProjectService } from "@/services/api";

export const useProjectStatus = (project_id: string) => {
  return useQuery({
    queryKey: queryKeys.settings.projectStatus(project_id),
    queryFn: () => ProjectService.getProjectStatus(project_id),
  });
};
