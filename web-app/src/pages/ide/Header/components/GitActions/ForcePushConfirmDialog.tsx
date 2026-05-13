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
  branch: string;
  aheadCount: number;
  isInConflict: boolean;
  isPending: boolean;
  onConfirm: () => void;
}

export function ForcePushConfirmDialog({
  open,
  onOpenChange,
  branch,
  aheadCount,
  isInConflict,
  isPending,
  onConfirm
}: Props) {
  const commitLabel = aheadCount === 1 ? "1 local commit" : `${aheadCount} local commits`;

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Force push to {branch}?</AlertDialogTitle>
          <AlertDialogDescription>
            {isInConflict
              ? `This overwrites the remote ${branch} branch with your local state, discarding any remote commits that aren't in your local history. Teammates working on this branch will need to reset. This cannot be undone.`
              : `This overwrites the remote ${branch} branch with ${commitLabel}, discarding any remote commits that aren't in your local history. Teammates working on this branch will need to reset. This cannot be undone.`}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            disabled={isPending}
            className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
          >
            {isPending ? "Pushing…" : "Force push"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
