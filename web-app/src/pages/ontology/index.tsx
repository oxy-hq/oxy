import { Loader2 } from "lucide-react";
import { ErrorAlert, ErrorAlertMessage } from "@/components/AppPreview/ErrorAlert";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import useOntology from "@/hooks/api/ontology/useOntology";
import { OntologyGraph } from "./OntologyGraph";

export default function OntologyPage() {
  const { data, isLoading, error } = useOntology();

  if (isLoading) {
    return (
      <div className='flex h-full items-center justify-center'>
        <div className='flex flex-col items-center gap-4'>
          <Loader2 className='h-8 w-8 animate-spin text-primary' />
          <p className='text-muted-foreground text-sm'>Loading ontology graph...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className='p-2'>
        <ErrorAlert>
          <ErrorAlertMessage>Error loading ontology</ErrorAlertMessage>
          <ErrorAlertMessage>
            {error instanceof Error ? error.message : "An unexpected error occurred"}
          </ErrorAlertMessage>
        </ErrorAlert>
      </div>
    );
  }

  if (!data || (data.nodes.length === 0 && data.edges.length === 0)) {
    return (
      <div className='flex h-full items-center justify-center p-4'>
        <Alert className='max-w-lg'>
          <AlertTitle>No data available</AlertTitle>
          <AlertDescription>
            The ontology graph is empty. Start by creating workflows, semantic models, or tables to
            see their relationships here.
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className='h-screen w-screen' data-testid='ontology-graph-container'>
      <OntologyGraph data={data} />
    </div>
  );
}
