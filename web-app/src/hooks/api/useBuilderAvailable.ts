import { useState, useEffect } from "react";
import { BuilderService } from "@/services/api";
import useCurrentProjectBranch from "../useCurrentProjectBranch";

/**
 * Hook to check if the builder agent is available
 * Checks if builder_agent is set in config.yml and if the value is a valid agent
 */
export default function useBuilderAvailable() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [isAvailable, setIsAvailable] = useState<boolean>(false);
  const [isLoading, setIsLoading] = useState<boolean>(true);

  useEffect(() => {
    const checkBuilderAvailability = async () => {
      try {
        setIsLoading(true);
        const result = await BuilderService.checkBuilderAvailability(projectId);
        setIsAvailable(result.available);
      } catch (error) {
        // If there's an error checking availability, assume builder is not available
        setIsAvailable(false);
        console.error("Error checking builder agent availability:", error);
      } finally {
        setIsLoading(false);
      }
    };

    checkBuilderAvailability();
  }, [projectId]);

  return { isAvailable, isLoading };
}
