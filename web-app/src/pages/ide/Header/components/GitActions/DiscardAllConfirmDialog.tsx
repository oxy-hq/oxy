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

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  uncommittedCount: number;
  isInConflict: boolean;
  isPending: boolean;
  onConfirm: () => void;
}

export function DiscardAllConfirmDialog({
  open,
  onOpenChange,
  uncommittedCount,
  isInConflict,
  isPending,
  onConfirm
}: Props) {
  const fileLabel = uncommittedCount === 1 ? "1 change" : `${uncommittedCount} changes`;

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Discard all local changes?</AlertDialogTitle>
          <AlertDialogDescription>
            {isInConflict
              ? "This aborts the in-progress rebase and removes all conflict markers and untracked files, restoring the branch to its state before the pull. This cannot be undone."
              : `This permanently removes ${fileLabel} — including untracked files — and restores the working tree to the latest commit on this branch. This cannot be undone.`}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            disabled={isPending}
            className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
          >
            {isPending ? "Discarding…" : "Discard changes"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
