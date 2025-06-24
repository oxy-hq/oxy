import React, { useCallback } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Hammer, Loader2, Zap } from "lucide-react";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import { useDataBuild } from "@/hooks/api/useDataBuild";

export const EmbeddingsManagement: React.FC = () => {
  const { isBuilding } = useDatabaseOperation();
  const buildMutation = useDataBuild();
  const buildingInProgress = isBuilding();

  const handleBuildEmbeddings = useCallback(() => {
    buildMutation.mutate();
  }, [buildMutation]);
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Zap className="h-5 w-5" />
          Embeddings Management
        </CardTitle>
        <CardDescription>
          Build embeddings for AI-powered search and analysis.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center justify-between">
          <Button onClick={handleBuildEmbeddings} disabled={buildingInProgress}>
            {buildingInProgress ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Building...
              </>
            ) : (
              <>
                <Hammer className="h-4 w-4 mr-2" />
                Build Embeddings
              </>
            )}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
};
