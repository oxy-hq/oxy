import { useCallback, useState } from "react";
import { Separator } from "@/components/ui/shadcn/separator";
import { Button } from "@/components/ui/shadcn/button";
import { Trash2, Loader2, Plus, Database } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/shadcn/dialog";
import DatabaseTable from "./DatabaseTable";
import { EmbeddingsManagement } from "./EmbeddingsManagement";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import { useDataClean } from "@/hooks/api/databases/useDataClean";
import { AddDatabaseForm } from "./AddDatabaseForm";
import PageHeader from "@/pages/ide/components/PageHeader";

export default function DatabasesPage() {
  const { isCleaning } = useDatabaseOperation();
  const cleanMutation = useDataClean();
  const cleaningInProgress = isCleaning();
  const [isAddDialogOpen, setIsAddDialogOpen] = useState(false);

  const handleCleanAll = useCallback(() => {
    cleanMutation.mutate("all");
  }, [cleanMutation]);

  const handleAddDatabaseSuccess = () => {
    setIsAddDialogOpen(false);
  };

  const handleCloseDialog = () => {
    setIsAddDialogOpen(false);
  };

  const listViewActions = (
    <div className="flex gap-2">
      <Button
        size="sm"
        variant="default"
        onClick={() => setIsAddDialogOpen(true)}
      >
        <Plus className="h-4 w-4" />
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
            <Loader2 className="h-4 w-4 animate-spin" />
            Resetting...
          </>
        ) : (
          <>
            <Trash2 className="h-4 w-4" />
            Reset Oxy State
          </>
        )}
      </Button>
    </div>
  );

  return (
    <div className="flex flex-col h-full">
      <PageHeader
        icon={Database}
        title="Databases"
        description="Manage database connections and embeddings"
        actions={listViewActions}
      />

      <div className="p-4 flex-1 overflow-auto min-h-0 customScrollbar scrollbar-gutter-auto">
        <DatabaseTable />

        <Separator className="my-6" />

        <EmbeddingsManagement />
      </div>

      <Dialog open={isAddDialogOpen} onOpenChange={setIsAddDialogOpen}>
        <DialogContent className="p-0 max-w-2xl max-h-[85vh] overflow-hidden flex flex-col">
          <DialogHeader className="p-6 pb-0">
            <DialogTitle>Add Database Connection</DialogTitle>
            <DialogDescription>
              Configure a new database connection
            </DialogDescription>
          </DialogHeader>
          <div className="p-6 pt-0 flex-1 overflow-auto min-h-0 customScrollbar">
            <AddDatabaseForm
              onSuccess={handleAddDatabaseSuccess}
              onCancel={handleCloseDialog}
            />
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
