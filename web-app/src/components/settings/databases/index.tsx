import { useCallback, useState } from "react";
import PageWrapper from "../components/PageWrapper";
import { Separator } from "@/components/ui/shadcn/separator";
import { Button } from "@/components/ui/shadcn/button";
import { Trash2, Loader2, Plus } from "lucide-react";
import DatabaseTable from "./DatabaseTable";
import { EmbeddingsManagement } from "./EmbeddingsManagement";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import { useDataClean } from "@/hooks/api/databases/useDataClean";
import { AddDatabaseForm } from "./AddDatabaseForm";

type View = "list" | "add";

const DatabaseManagement = () => {
  const { isCleaning } = useDatabaseOperation();
  const cleanMutation = useDataClean();
  const cleaningInProgress = isCleaning();
  const [currentView, setCurrentView] = useState<View>("list");

  const handleCleanAll = useCallback(() => {
    cleanMutation.mutate("all");
  }, [cleanMutation]);

  const handleAddDatabaseSuccess = () => {
    setCurrentView("list");
  };

  const handleBack = () => {
    setCurrentView("list");
  };

  const listViewActions = (
    <div className="flex gap-2">
      <Button size="sm" variant="default" onClick={() => setCurrentView("add")}>
        <Plus />
        Add Database
      </Button>
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
    </div>
  );

  if (currentView === "add") {
    return (
      <PageWrapper title="Add Database Connection" onBack={handleBack}>
        <AddDatabaseForm
          onSuccess={handleAddDatabaseSuccess}
          onCancel={handleBack}
        />
      </PageWrapper>
    );
  }

  return (
    <PageWrapper title="Database Management" actions={listViewActions}>
      <DatabaseTable />

      <Separator className="my-6" />

      <EmbeddingsManagement />
    </PageWrapper>
  );
};

export default DatabaseManagement;
