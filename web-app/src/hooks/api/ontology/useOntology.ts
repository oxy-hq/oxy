import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { OntologyService } from "@/services/api/ontology";
import queryKeys from "../queryKey";

const useOntology = () => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.ontology.graph(project.id, branchName),
    queryFn: () => OntologyService.getOntologyGraph(project.id, branchName)
  });
};

export default useOntology;
