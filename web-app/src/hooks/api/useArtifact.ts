import { useQuery } from "@tanstack/react-query";
import { service } from "@/services/service";

export const useArtifact = (id: string) => {
  return useQuery({
    queryKey: ["artifact", id],
    queryFn: () => service.getArtifact(id),
  });
};
