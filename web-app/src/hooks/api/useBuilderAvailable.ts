import { useEffect, useState } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { BuilderService } from "@/services/api";

/**
 * Hook to check if the builder agent is available.
 * Supports both legacy path-based agents and the new built-in copilot.
 */
export default function useBuilderAvailable() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [isAvailable, setIsAvailable] = useState<boolean>(false);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [builderPath, setBuilderPath] = useState<string>("");
  const [isBuiltin, setIsBuiltin] = useState<boolean>(false);
  const [builderModel, setBuilderModel] = useState<string | undefined>(undefined);
  const isAgentic = builderPath.endsWith(".aw.yaml") || builderPath.endsWith(".aw.yml");

  useEffect(() => {
    const checkBuilderAvailability = async () => {
      try {
        setIsLoading(true);
        const result = await BuilderService.checkBuilderAvailability(projectId);
        setIsAvailable(result.available);
        setBuilderPath(result.builder_path || "");
        setIsBuiltin(result.builtin ?? false);
        setBuilderModel(result.model ?? undefined);
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

  return { isAvailable, isAgentic, isLoading, builderPath, isBuiltin, builderModel };
}
