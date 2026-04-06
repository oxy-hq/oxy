import ErrorAlert from "@/components/ui/ErrorAlert";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useContextGraph from "@/hooks/api/contextGraph/useContextGraph";
import { ContextGraph } from "./ContextGraph";

export default function ContextGraphPage() {
  const { data, isLoading, error } = useContextGraph();

  if (isLoading) {
    return (
      <div className='flex h-full items-center justify-center'>
        <div className='flex flex-col items-center gap-4'>
          <Spinner className='size-8 text-primary' />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className='mx-auto w-full max-w-page-content p-2'>
        <ErrorAlert
          title='Error loading context graph'
          message={error instanceof Error ? error.message : "An unexpected error occurred"}
        />
      </div>
    );
  }

  if (!data || (data.nodes.length === 0 && data.edges.length === 0)) {
    return (
      <div className='flex h-full items-center justify-center p-4'>
        <Alert className='max-w-lg'>
          <AlertTitle>No data available</AlertTitle>
          <AlertDescription>
            The context graph is empty. Start by creating workflows, semantic models, or tables to
            see their relationships here.
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className='h-screen w-screen' data-testid='context-graph-container'>
      <ContextGraph data={data} />
    </div>
  );
}
