import { useState } from "react";
import { useParams } from "react-router-dom";
import { useProjects } from "@/hooks/api/projects/useProjects";
import { useProjectOperations } from "@/hooks/useProjectOperations";
import { useProjectsFilter } from "@/hooks/useProjectsFilter";
import NewProjectDialog from "./new_project_dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Search } from "lucide-react";
import Content from "./content";
import Header from "./Header";

export default function ProjectsPage() {
  const { organizationId } = useParams<{ organizationId: string }>();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");

  const {
    data: projectsData,
    isPending,
    error,
    refetch,
  } = useProjects(organizationId!);

  const { handleDeleteProject, handleProjectClick, isDeleting } =
    useProjectOperations(organizationId!);

  const filteredProjects = useProjectsFilter(
    projectsData?.projects,
    searchQuery,
  );

  const handleClearSearch = () => setSearchQuery("");

  return (
    <div className="flex flex-col w-full">
      <Header />

      <div className="flex-1 p-6 max-w-6xl mx-auto max-w-[1200px] w-full">
        <div className="mb-6">
          <h1 className="text-2xl mb-4">Projects</h1>
          <div className="flex items-center justify-between">
            <div className="flex items-center">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder="Search for a project"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-10 w-80"
                />
              </div>
            </div>
            <NewProjectDialog
              isOpen={isModalOpen}
              onOpenChange={setIsModalOpen}
            />
          </div>
        </div>

        <div className="space-y-6">
          <Content
            projects={projectsData?.projects}
            filteredProjects={filteredProjects}
            searchQuery={searchQuery}
            isPending={isPending}
            error={error}
            onProjectClick={handleProjectClick}
            onDeleteProject={handleDeleteProject}
            onClearSearch={handleClearSearch}
            onRetry={refetch}
            isDeleting={isDeleting}
          />
        </div>
      </div>
    </div>
  );
}
