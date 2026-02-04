import { Database, Loader2, Plus, Trash2 } from "lucide-react";
import { useCallback, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Separator } from "@/components/ui/shadcn/separator";
import { useDataClean } from "@/hooks/api/databases/useDataClean";
import PageHeader from "@/pages/ide/components/PageHeader";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import { AddDatabaseForm } from "./AddDatabaseForm";
import DatabaseTable from "./DatabaseTable";
import { EmbeddingsManagement } from "./EmbeddingsManagement";

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
    <div className='flex gap-2'>
      <Button size='sm' variant='default' onClick={() => setIsAddDialogOpen(true)}>
        <Plus className='h-4 w-4' />
        Add Database
      </Button>
      <Button size='sm' variant='outline' onClick={handleCleanAll} disabled={cleaningInProgress}>
        {cleaningInProgress ? (
          <>
            <Loader2 className='h-4 w-4 animate-spin' />
            Resetting...
          </>
        ) : (
          <>
            <Trash2 className='h-4 w-4' />
            Reset Oxy State
          </>
        )}
      </Button>
    </div>
  );

  return (
    <div className='flex h-full flex-col'>
      <PageHeader
        icon={Database}
        title='Databases'
        description='Manage database connections and embeddings'
        actions={listViewActions}
      />

      <div className='customScrollbar scrollbar-gutter-auto min-h-0 flex-1 overflow-auto p-4'>
        <DatabaseTable />

        <Separator className='my-6' />

        <EmbeddingsManagement />
      </div>

      <Dialog open={isAddDialogOpen} onOpenChange={setIsAddDialogOpen}>
        <DialogContent className='flex max-h-[85vh] max-w-2xl flex-col overflow-hidden p-0'>
          <DialogHeader className='p-6 pb-0'>
            <DialogTitle>Add Database Connection</DialogTitle>
            <DialogDescription>Configure a new database connection</DialogDescription>
          </DialogHeader>
          <div className='customScrollbar min-h-0 flex-1 overflow-auto p-6 pt-0'>
            <AddDatabaseForm onSuccess={handleAddDatabaseSuccess} onCancel={handleCloseDialog} />
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
