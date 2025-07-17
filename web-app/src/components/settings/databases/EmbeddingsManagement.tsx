import React, { useCallback } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Hammer, Loader2 } from "lucide-react";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import { useDataBuild } from "@/hooks/api/databases/useDataBuild";
import { Label } from "@/components/ui/shadcn/label";

export const EmbeddingsManagement: React.FC = () => {
  const { isBuilding } = useDatabaseOperation();
  const buildMutation = useDataBuild();
  const buildingInProgress = isBuilding();

  const handleBuildEmbeddings = useCallback(() => {
    buildMutation.mutate();
  }, [buildMutation]);

  return (
    <div className="flex items-start justify-between">
      <div className="space-y-1">
        <Label className="text-sm">AI & Embeddings</Label>
        <p className="text-sm text-muted-foreground">
          Build embeddings for AI-powered search and analysis.
        </p>
      </div>
      <Button
        size="sm"
        onClick={handleBuildEmbeddings}
        disabled={buildingInProgress}
      >
        {buildingInProgress ? (
          <>
            <Loader2 className="animate-spin" />
            Building...
          </>
        ) : (
          <>
            <Hammer />
            Build Embeddings
          </>
        )}
      </Button>
    </div>
  );
};
