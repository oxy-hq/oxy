import { OntologyGraph } from "./OntologyGraph";
import useOntology from "@/hooks/api/ontology/useOntology";
import { Loader2 } from "lucide-react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/shadcn/alert";

export default function OntologyPage() {
  const { data, isLoading, error } = useOntology();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="flex flex-col items-center gap-4">
          <Loader2 className="w-8 h-8 animate-spin text-primary" />
          <p className="text-sm text-muted-foreground">
            Loading ontology graph...
          </p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-4">
        <Alert variant="destructive" className="max-w-lg">
          <AlertTitle>Error loading ontology</AlertTitle>
          <AlertDescription>
            {error instanceof Error
              ? error.message
              : "An unexpected error occurred"}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (!data || (data.nodes.length === 0 && data.edges.length === 0)) {
    return (
      <div className="flex items-center justify-center h-full p-4">
        <Alert className="max-w-lg">
          <AlertTitle>No data available</AlertTitle>
          <AlertDescription>
            The ontology graph is empty. Start by creating workflows, semantic
            models, or tables to see their relationships here.
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="h-screen w-screen" data-testid="ontology-graph-container">
      <OntologyGraph data={data} />
    </div>
  );
}
