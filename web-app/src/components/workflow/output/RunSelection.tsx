import React from "react";
import { useListWorkflowRuns, useDeleteWorkflowRun } from "../useWorkflowRun";
import { get } from "lodash";
import { RunInfo } from "@/services/types/runs";
import { createSearchParams, useLocation, useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Trash2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/shadcn/alert-dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { toast } from "sonner";

interface Props {
  workflowId: string;
  runId?: string;
}

const RunSelection: React.FC<Props> = ({ workflowId, runId }) => {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();
  const [runToDelete, setRunToDelete] = React.useState<number | null>(null);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);

  const deleteRun = useDeleteWorkflowRun();

  const onRunIdChange = (newRunId: string) => {
    navigate({
      pathname: location.pathname,
      search: createSearchParams({
        run: newRunId.toString(),
      }).toString(),
    });
  };

  const { data, isPending } = useListWorkflowRuns(workflowId, {
    pageIndex: 0,
    pageSize: 10000,
  });

  const items = get(data, "items", []);

  const handleDeleteClick = (runIndex: number) => {
    setRunToDelete(runIndex);
    setShowDeleteDialog(true);
  };

  const handleDelete = async () => {
    if (runToDelete === null) return;

    try {
      await deleteRun.mutateAsync({
        workflowId,
        runIndex: runToDelete,
      });
      toast.success("Run deleted successfully");

      // Invalidate the runs list query to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.workflow.getRuns(
          project.id,
          branchName,
          workflowId,
          { pageIndex: 0, pageSize: 10000 },
        ),
      });

      // If the currently viewed run was deleted, navigate to the first available run
      if (runId && parseInt(runId) === runToDelete) {
        const remainingRuns = items.filter(
          (run: RunInfo) => run.run_index !== runToDelete,
        );
        if (remainingRuns.length > 0) {
          onRunIdChange(remainingRuns[0].run_index.toString());
        } else {
          navigate(location.pathname);
        }
      }

      setRunToDelete(null);
      setShowDeleteDialog(false);
    } catch (error) {
      toast.error("Failed to delete run");
      console.error("Error deleting run:", error);
    }
  };

  return (
    <>
      <div className="flex items-center gap-2">
        <Select value={runId} onValueChange={onRunIdChange}>
          <SelectTrigger className="w-[280px]">
            <SelectValue placeholder="Select the run" />
          </SelectTrigger>
          <SelectContent>
            {isPending && <div className="p-4">Loading...</div>}
            {!isPending && items.length === 0 && (
              <div className="p-4 text-sm text-muted-foreground">
                No runs available
              </div>
            )}
            {items.map((run: RunInfo) => (
              <SelectItem
                key={run.run_index}
                value={run.run_index.toString()}
                className="group"
              >
                <div className="flex items-center justify-between w-full gap-2">
                  <div className="flex items-center gap-3 flex-1 min-w-0">
                    <p className="text-sm font-medium whitespace-nowrap">
                      Run {run.run_index}
                    </p>
                    <p className="text-xs text-muted-foreground whitespace-nowrap overflow-hidden text-ellipsis">
                      {new Date(run.updated_at).toLocaleDateString()}{" "}
                      {new Date(run.updated_at).toLocaleTimeString([], {
                        hour: "2-digit",
                        minute: "2-digit",
                      })}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onPointerDown={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      handleDeleteClick(run.run_index);
                    }}
                    className="h-7 w-7 p-0 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
                  >
                    <Trash2 className="w-3.5 h-3.5 text-destructive" />
                  </Button>
                </div>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete workflow run?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete run {runToDelete}. This action cannot
              be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel
              onClick={() => {
                setRunToDelete(null);
              }}
            >
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              className="bg-destructive text-white hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default RunSelection;
