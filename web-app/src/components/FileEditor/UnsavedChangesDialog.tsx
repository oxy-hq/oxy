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
import { Button } from "@/components/ui/shadcn/button";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: () => void;
  onDiscard: () => void;
  /** When true, shows branch-creation language instead of generic "save" language. */
  isMainEditMode?: boolean;
}

const UnsavedChangesDialog = ({ open, onOpenChange, onSave, onDiscard, isMainEditMode }: Props) => {
  const title = isMainEditMode
    ? "Save changes to a new branch?"
    : "Do you want to save the changes you made to this file?";

  const description = isMainEditMode
    ? "You're editing on main. Your changes will be saved to a new branch automatically."
    : "Your changes will be lost if you don't save them.";

  const saveLabel = isMainEditMode ? "Save to new branch" : "Save";

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className='gap-8'>
          <div className='flex gap-2'>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <Button variant='secondary' onClick={onDiscard}>
              Discard
            </Button>
            <AlertDialogAction onClick={onSave}>{saveLabel}</AlertDialogAction>
          </div>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

export default UnsavedChangesDialog;
