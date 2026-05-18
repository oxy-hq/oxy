import { Database, Plus } from "lucide-react";
import type React from "react";
import { CanWorkspaceEditor } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { cn } from "@/libs/shadcn/utils";
import type { PipelineFile } from "../usePipelineFiles";

interface PipelineListProps {
  pipelines: PipelineFile[];
  isLoading: boolean;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  onNew: () => void;
}

const PipelineList: React.FC<PipelineListProps> = ({
  pipelines,
  isLoading,
  selectedPath,
  onSelect,
  onNew
}) => (
  <div className='flex h-full flex-col border-r bg-sidebar-background'>
    <div className='flex items-center justify-between border-b px-3 py-2'>
      <span className='font-medium text-sm'>Pipelines</span>
      <CanWorkspaceEditor>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6'
          onClick={onNew}
          tooltip={{ content: "New pipeline", side: "right" }}
        >
          <Plus className='h-3.5 w-3.5' />
        </Button>
      </CanWorkspaceEditor>
    </div>
    <div className='flex-1 overflow-y-auto py-1'>
      {isLoading ? (
        <div className='flex items-center justify-center py-6'>
          <Spinner className='h-4 w-4' />
        </div>
      ) : pipelines.length === 0 ? (
        <div className='px-3 py-6 text-center text-muted-foreground text-xs'>
          No pipelines yet. Create one to get started.
        </div>
      ) : (
        pipelines.map((p) => (
          <button
            type='button'
            key={p.path}
            onClick={() => onSelect(p.path)}
            className={cn(
              "flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm",
              p.path === selectedPath
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "hover:bg-sidebar-accent/50"
            )}
          >
            <Database className='h-3.5 w-3.5 shrink-0 opacity-70' />
            <span className='min-w-0 truncate'>{p.name}</span>
          </button>
        ))
      )}
    </div>
  </div>
);

export default PipelineList;
