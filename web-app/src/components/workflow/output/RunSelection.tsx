import { useQuery, useQueryClient } from "@tanstack/react-query";
import { get } from "lodash";
import { Trash2 } from "lucide-react";
import React from "react";
import { createSearchParams, useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/shadcn/avatar";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { UserService } from "@/services/api/users";
import type { RunInfo } from "@/services/types/runs";
import type { UserInfo } from "@/types/auth";
import { useDeleteWorkflowRun, useListWorkflowRuns } from "../useWorkflowRun";

interface Props {
  workflowId: string;
  runId?: string;
}

const RunSelection: React.FC<Props> = ({ workflowId, runId }) => {
  const location = useLocation();
  const navigate = useNavigate();

  const { data, isPending } = useListWorkflowRuns(workflowId, {
    pageIndex: 0,
    pageSize: 10000
  });

  const items = get(data, "items", []);

  // Extract unique user_ids from runs
  const userIds = React.useMemo(() => {
    const ids = items.map((run: RunInfo) => run.user_id).filter((id): id is string => id != null);
    return Array.from(new Set(ids));
  }, [items]);

  // Fetch users by IDs
  const { data: usersData } = useQuery({
    queryKey: ["users", "batch", userIds],
    queryFn: () => UserService.batchGetUsers(userIds),
    enabled: userIds.length > 0,
    staleTime: 5 * 60 * 1000 // Cache for 5 minutes
  });

  // Create a map of user_id to user info for quick lookup
  const usersMap = React.useMemo(() => {
    const map = new Map<string, UserInfo>();
    if (usersData?.users) {
      usersData.users.forEach((user) => {
        map.set(user.id, user);
      });
    }
    return map;
  }, [usersData]);

  const queryClient = useQueryClient();
  const { project: projectBranch, branchName } = useCurrentProjectBranch();
  const [runToDelete, setRunToDelete] = React.useState<number | null>(null);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);

  const deleteRun = useDeleteWorkflowRun();

  const onRunIdChange = (newRunId: string) => {
    navigate({
      pathname: location.pathname,
      search: createSearchParams({
        run: newRunId.toString()
      }).toString()
    });
  };

  const handleDeleteClick = (runIndex: number) => {
    setRunToDelete(runIndex);
    setShowDeleteDialog(true);
  };

  const handleDelete = async () => {
    if (runToDelete === null) return;

    try {
      await deleteRun.mutateAsync({
        workflowId,
        runIndex: runToDelete
      });
      toast.success("Run deleted successfully");

      // Invalidate the runs list query to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.workflow.getRuns(projectBranch.id, branchName, workflowId, {
          pageIndex: 0,
          pageSize: 10000
        })
      });

      // If the currently viewed run was deleted, navigate to the first available run
      if (runId && parseInt(runId, 10) === runToDelete) {
        const remainingRuns = items.filter((run: RunInfo) => run.run_index !== runToDelete);
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
      <Select value={runId} onValueChange={onRunIdChange}>
        <SelectTrigger>
          <SelectValue placeholder='Select the run' />
        </SelectTrigger>
        <SelectContent>
          {isPending && <div className='p-4'>Loading...</div>}
          {items.map((run: RunInfo) => {
            const user = run.user_id ? usersMap.get(run.user_id) : null;

            // Show user email if available, otherwise show truncated user_id
            const displayName =
              user?.email || (run.user_id ? `User ${run.user_id.slice(0, 8)}...` : "Unknown");
            const avatarFallback =
              user?.name?.charAt(0).toUpperCase() ||
              user?.email?.charAt(0).toUpperCase() ||
              (run.user_id ? run.user_id.charAt(0).toUpperCase() : "?");

            return (
              <SelectItem key={run.run_index} value={run.run_index.toString()}>
                <div className='flex w-full items-center justify-between gap-2'>
                  <TooltipProvider>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <div className='flex min-w-0 flex-1 items-center gap-2'>
                          <Avatar className='h-5 w-5 flex-shrink-0'>
                            <AvatarImage src={user?.picture} alt={displayName} />
                            <AvatarFallback className='text-xs'>{avatarFallback}</AvatarFallback>
                          </Avatar>
                          <span className='flex-shrink-0 font-medium text-sm'>
                            Run {run.run_index}
                          </span>
                          <span className='flex-shrink-0 text-muted-foreground text-xs'>
                            {new Date(run.updated_at).toLocaleTimeString()}
                          </span>
                        </div>
                      </TooltipTrigger>
                      <TooltipContent>
                        <p>{displayName}</p>
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5 flex-shrink-0 hover:bg-destructive/10 hover:text-destructive'
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteClick(run.run_index);
                    }}
                  >
                    <Trash2 className='h-3 w-3' />
                  </Button>
                </div>
              </SelectItem>
            );
          })}
        </SelectContent>
      </Select>

      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Workflow Run</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete this workflow run? This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
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
