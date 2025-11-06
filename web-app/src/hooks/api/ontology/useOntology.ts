import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { OntologyService } from "@/services/api/ontology";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useOntology = () => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.ontology.graph(project.id, branchName),
    queryFn: () => OntologyService.getOntologyGraph(project.id, branchName),
  });
};

export default useOntology;
