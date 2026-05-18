/**
 * Pipelines Developer Portal section (`/ide/pipelines`).
 *
 * Modeled on the Modeling/airform area: a master list of `.airway.yml`
 * pipelines on the left, the run/monitor view (the existing
 * `AirwayRunPage`, embedded) on the right. "New pipeline" scaffolds a
 * spec via a small form and drops the user into the YAML editor to
 * fill in credentials.
 */

import { FileCog } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import { AirwayPipelinePage, AirwayRunDetailPage } from "@/pages/airway";
import useCurrentOrg from "@/stores/useCurrentOrg";
import NewPipelineDialog from "./components/NewPipelineDialog";
import PipelineList from "./components/PipelineList";
import usePipelineFiles from "./usePipelineFiles";

const PipelinesPage: React.FC = () => {
  const { pipelines, isLoading, refetch } = usePipelineFiles();
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  // Embedded master-detail: which run (if any) is open for the
  // selected pipeline. Cleared when the pipeline selection changes.
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const navigate = useNavigate();

  const selectedPathb64 = selectedPath ? encodeBase64(selectedPath) : null;

  return (
    <div className='flex h-full flex-1 flex-col overflow-hidden'>
      <ResizablePanelGroup direction='horizontal' className='flex-1'>
        <ResizablePanel defaultSize={25} minSize={15} className='min-w-[200px]'>
          <PipelineList
            pipelines={pipelines}
            isLoading={isLoading}
            selectedPath={selectedPath}
            onSelect={(p) => {
              setSelectedPath(p);
              setSelectedRunId(null);
            }}
            onNew={() => setDialogOpen(true)}
          />
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel defaultSize={75} minSize={30}>
          {selectedPath && selectedPathb64 ? (
            <div className='flex h-full flex-col'>
              <div className='flex items-center gap-2 border-b px-4 py-2'>
                <span className='min-w-0 truncate font-medium text-sm'>{selectedPath}</span>
                <Button
                  variant='outline'
                  size='sm'
                  className='ml-auto'
                  onClick={() =>
                    navigate(
                      ROUTES.ORG(orgSlug).WORKSPACE(project.id).IDE.FILES.FILE(selectedPathb64)
                    )
                  }
                >
                  <FileCog className='h-4 w-4' />
                  Edit YAML
                </Button>
              </div>
              <div className='min-h-0 flex-1'>
                {selectedRunId ? (
                  <AirwayRunDetailPage
                    key={`${selectedPathb64}:${selectedRunId}`}
                    pathb64={selectedPathb64}
                    runId={selectedRunId}
                    hideHeader
                    onBack={() => setSelectedRunId(null)}
                    onOpenRun={setSelectedRunId}
                  />
                ) : (
                  <AirwayPipelinePage
                    key={selectedPathb64}
                    pathb64={selectedPathb64}
                    hideHeader
                    onOpenRun={setSelectedRunId}
                  />
                )}
              </div>
            </div>
          ) : (
            <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
              Select a pipeline, or create one to get started.
            </div>
          )}
        </ResizablePanel>
      </ResizablePanelGroup>

      <NewPipelineDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        existingNames={pipelines.map((p) => p.name)}
        onCreated={refetch}
      />
    </div>
  );
};

export default PipelinesPage;
