import PageHeader from "@/components/PageHeader";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useContextGraph from "@/hooks/api/contextGraph/useContextGraph";
import { ContextGraph } from "./ContextGraph";

export default function ContextGraphPage() {
  const { data, isLoading, error } = useContextGraph();

  return (
    <div className='flex h-full w-full flex-col'>
      <PageHeader />
      <div className='relative min-h-0 flex-1' data-testid='context-graph-container'>
        {isLoading && (
          <div className='flex h-full items-center justify-center'>
            <Spinner className='size-8 text-primary' />
          </div>
        )}

        {!isLoading && error && (
          <div className='mx-auto w-full max-w-page-content p-2'>
            <ErrorAlert
              title='Error loading context graph'
              message={error instanceof Error ? error.message : "An unexpected error occurred"}
            />
          </div>
        )}

        {!isLoading &&
          !error &&
          (!data || (data.nodes.length === 0 && data.edges.length === 0)) && (
            <div className='flex h-full items-center justify-center p-4'>
              <Alert className='max-w-lg'>
                <AlertTitle>No data available</AlertTitle>
                <AlertDescription>
                  The context graph is empty. Start by creating automations, semantic models, or
                  tables to see their relationships here.
                </AlertDescription>
              </Alert>
            </div>
          )}

        {!isLoading && !error && data && (data.nodes.length > 0 || data.edges.length > 0) && (
          <ContextGraph data={data} />
        )}
      </div>
    </div>
  );
}
