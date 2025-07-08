import { useQuery } from "@tanstack/react-query";
import queryKeys from "./api/queryKey";
import { ProjectService } from "@/services/projectService";

export const useProjectStatus = () => {
  return useQuery({
    queryKey: queryKeys.settings.projectStatus(),
    queryFn: () => ProjectService.getProjectStatus(),
  });
};
