import { Trash2 } from "lucide-react";
import type React from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";

interface Props {
  selectedCount: number;
  totalAcrossAll?: number;
  isSelectAllPages: boolean;
  onBulkDelete: () => void;
  isLoading?: boolean;
}

const DeleteAction: React.FC<Props> = ({
  selectedCount,
  totalAcrossAll,
  isSelectAllPages,
  onBulkDelete,
  isLoading = false
}) => {
  const threadsToDelete = isSelectAllPages && totalAcrossAll ? totalAcrossAll : selectedCount;

  const threadText = threadsToDelete === 1 ? "thread" : "threads";
  const description =
    isSelectAllPages && totalAcrossAll
      ? `Are you sure you want to delete all ${totalAcrossAll} threads? This action cannot be undone.`
      : `Are you sure you want to delete ${threadsToDelete} ${threadText}? This action cannot be undone.`;

  return (
    <AlertDialog>
      <AlertDialogTrigger asChild>
        <Button variant='destructive' size='sm' disabled={isLoading}>
          <Trash2 className='h-4 w-4' />
          Delete {threadsToDelete}
        </Button>
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete threads?</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isLoading}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onBulkDelete}
            disabled={isLoading}
            className={buttonVariants({ variant: "destructive" })}
          >
            {isLoading ? "Deleting..." : "Delete"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

export default DeleteAction;
