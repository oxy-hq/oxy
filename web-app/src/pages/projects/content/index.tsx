import { Project } from "@/types/project";
import EmptyState from "./EmptyState";
import LoadingState from "./LoadingState";
import ProjectCard from "./ProjectCard";

interface Props {
  projects?: Project[];
  filteredProjects: Project[];
  searchQuery: string;
  isPending: boolean;
  error: Error | null;
  onProjectClick: (project: Project) => void;
  onDeleteProject: (projectId: string, projectName: string) => void;
  onClearSearch: () => void;
  onRetry: () => void;
  isDeleting: boolean;
}

const Content = ({
  projects,
  filteredProjects,
  searchQuery,
  isPending,
  error,
  onProjectClick,
  onDeleteProject,
  onClearSearch,
  onRetry,
  isDeleting,
}: Props) => {
  if (isPending) {
    return <LoadingState />;
  }

  if (error) {
    return <LoadingState error={error} onRetry={onRetry} />;
  }

  if (!projects || projects.length === 0) {
    return <EmptyState type="no-projects" />;
  }

  if (filteredProjects.length === 0 && searchQuery.trim()) {
    return (
      <EmptyState type="no-search-results" onClearSearch={onClearSearch} />
    );
  }

  return (
    <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
      {projects.map((project) => (
        <ProjectCard
          key={project.id}
          project={project}
          onProjectClick={onProjectClick}
          onDeleteProject={onDeleteProject}
          isDeleting={isDeleting}
        />
      ))}
    </div>
  );
};

export default Content;
