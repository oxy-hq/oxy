import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { useDeleteProject } from "@/hooks/api/projects/useProjects";
import { ProjectService } from "@/services/api";
import { Project } from "@/types/project";
import ROUTES from "@/libs/utils/routes";

export const useProjectOperations = (organizationId: string) => {
  const navigate = useNavigate();
  const deleteProjectMutation = useDeleteProject(organizationId);

  const handleDeleteProject = async (
    projectId: string,
    projectName: string,
  ) => {
    try {
      await deleteProjectMutation.mutateAsync(projectId);
      toast.success(`Project "${projectName}" deleted successfully!`);
    } catch (error) {
      toast.error("Failed to delete project");
      console.error("Error deleting project:", error);
    }
  };

  const handleProjectClick = async (project: Project) => {
    try {
      const projectStatus = await ProjectService.getProjectStatus(
        project.id,
        project.active_branch?.name ?? "",
      );

      const requiredSecrets = projectStatus?.required_secrets || [];

      if (requiredSecrets.length > 0) {
        navigate(ROUTES.PROJECT(project.id).REQUIRED_SECRETS, {
          replace: true,
        });
        return;
      }

      navigate(ROUTES.PROJECT(project.id).ROOT);
    } catch (error) {
      console.error("Error fetching project status:", error);
      // Still navigate to project even if status check fails
      navigate(ROUTES.PROJECT(project.id).ROOT);
    }
  };

  return {
    handleDeleteProject,
    handleProjectClick,
    isDeleting: deleteProjectMutation.isPending,
  };
};
