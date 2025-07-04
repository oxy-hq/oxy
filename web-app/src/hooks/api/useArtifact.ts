import { useQuery } from "@tanstack/react-query";
import { ArtifactService } from "@/services/api";

export const useArtifact = (id: string) => {
  return useQuery({
    queryKey: ["artifact", id],
    queryFn: () => ArtifactService.getArtifact(id),
  });
};
