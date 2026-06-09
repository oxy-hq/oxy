import { useState } from "react";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { useMetricTree } from "@/hooks/api/useMetricTree";
import { DefinitionPanel } from "./components/DefinitionPanel";
import { MetricTreeGraph } from "./components/MetricTreeGraph";
import { SensitivityPanel } from "./components/SensitivityPanel";

/**
 * Metric Tree view — the workspace's measures as an interactive graph, with
 * a Drivers (sensitivity) side panel for the selected measure.
 */
export default function MetricTreeView() {
  const { data: tree, isLoading, error } = useMetricTree();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  if (isLoading) {
    return (
      <div className='flex h-full items-center justify-center'>
        <Spinner />
      </div>
    );
  }
  if (error) {
    return (
      <div className='flex h-full items-center justify-center text-destructive text-sm'>
        {error instanceof Error ? error.message : "Failed to load the metric tree."}
      </div>
    );
  }
  if (!tree) return null;

  return (
    <div className='flex h-full min-h-0 flex-1 overflow-hidden' data-testid='metric-tree-view'>
      <div className='h-full min-w-0 flex-1'>
        <MetricTreeGraph tree={tree} selectedId={selectedId} onSelect={setSelectedId} />
      </div>
      <div className='flex w-144 flex-col border-border border-l'>
        <Tabs defaultValue='definition' className='flex min-h-0 flex-1 flex-col gap-0'>
          <div className='border-border border-b px-4 py-2'>
            <TabsList className='w-fit'>
              <TabsTrigger value='definition'>Definition</TabsTrigger>
              <TabsTrigger value='drivers'>Drivers</TabsTrigger>
            </TabsList>
          </div>
          <TabsContent value='definition' className='min-h-0 flex-1 overflow-auto'>
            <DefinitionPanel measureId={selectedId} tree={tree} />
          </TabsContent>
          <TabsContent value='drivers' className='min-h-0 flex-1 overflow-auto'>
            <SensitivityPanel measureId={selectedId} />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}
