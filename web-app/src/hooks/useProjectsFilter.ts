import { useMemo } from "react";
import { Project } from "@/types/project";

export const useProjectsFilter = (
  projects: Project[] | undefined,
  searchQuery: string,
) => {
  const filteredProjects = useMemo(() => {
    if (!projects) return [];

    if (!searchQuery.trim()) return projects;

    return projects.filter((project) =>
      project.name.toLowerCase().includes(searchQuery.toLowerCase()),
    );
  }, [projects, searchQuery]);

  return filteredProjects;
};
