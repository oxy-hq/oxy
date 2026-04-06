import { Hammer, Trash2 } from "lucide-react";
import type React from "react";
import { useCallback } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useDataBuild } from "@/hooks/api/databases/useDataBuild";
import { useDataClean } from "@/hooks/api/databases/useDataClean";
import useDatabaseOperation from "@/stores/useDatabaseOperation";

export const EmbeddingsManagement: React.FC = () => {
  const { isBuilding, isCleaning } = useDatabaseOperation();
  const buildMutation = useDataBuild();
  const cleanMutation = useDataClean();
  const buildingInProgress = isBuilding();
  const cleaningInProgress = isCleaning();

  const handleBuildEmbeddings = useCallback(() => {
    buildMutation.mutate();
  }, [buildMutation]);

  const handleCleanData = useCallback(() => {
    cleanMutation.mutate("vectors");
  }, [cleanMutation]);

  return (
    <div className='flex items-start justify-between'>
      <div className='space-y-1'>
        <Label className='text-sm'>AI & Embeddings</Label>
        <p className='text-muted-foreground text-sm'>
          Build embeddings for AI-powered search and analysis, or clean embeddings data.
        </p>
      </div>
      <div className='flex gap-2'>
        <Button
          size='sm'
          variant='destructive'
          onClick={handleCleanData}
          disabled={cleaningInProgress || buildingInProgress}
        >
          {cleaningInProgress ? (
            <Spinner />
          ) : (
            <>
              <Trash2 />
              Clean
            </>
          )}
        </Button>
        <Button
          size='sm'
          onClick={handleBuildEmbeddings}
          disabled={buildingInProgress || cleaningInProgress}
        >
          {buildingInProgress ? (
            <Spinner />
          ) : (
            <>
              <Hammer />
              Build
            </>
          )}
        </Button>
      </div>
    </div>
  );
};
