import { useCallback } from "react";
import PageWrapper from "../components/PageWrapper";
import { Separator } from "@/components/ui/shadcn/separator";
import { Button } from "@/components/ui/shadcn/button";
import { Trash2, Loader2 } from "lucide-react";
import DatabaseTable from "./DatabaseTable";
import { EmbeddingsManagement } from "./EmbeddingsManagement";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import { useDataClean } from "@/hooks/api/databases/useDataClean";

const DatabaseManagement = () => {
  const { isCleaning } = useDatabaseOperation();
  const cleanMutation = useDataClean();
  const cleaningInProgress = isCleaning();

  const handleCleanAll = useCallback(() => {
    cleanMutation.mutate("all");
  }, [cleanMutation]);

  const cleanButton = (
    <Button
      size="sm"
      variant="outline"
      onClick={handleCleanAll}
      disabled={cleaningInProgress}
    >
      {cleaningInProgress ? (
        <>
          <Loader2 className="animate-spin" />
          Resetting...
        </>
      ) : (
        <>
          <Trash2 />
          Reset Oxy State
        </>
      )}
    </Button>
  );

  return (
    <PageWrapper title="Database Management" actions={cleanButton}>
      <DatabaseTable />

      <Separator className="my-6" />

      <EmbeddingsManagement />
    </PageWrapper>
  );
};

export default DatabaseManagement;
