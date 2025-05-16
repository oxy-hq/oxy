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
import { Button } from "@/components/ui/shadcn/button";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: () => void;
  onDiscard: () => void;
}

const UnsavedChangesDialog = ({
  open,
  onOpenChange,
  onSave,
  onDiscard,
}: Props) => {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            Do you want to save the changes you made to this file?
          </AlertDialogTitle>
          <AlertDialogDescription>
            Your changes will be lost if you don't save them.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className="gap-8">
          <div className="flex gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <Button variant="secondary" onClick={onDiscard}>
              Discard
            </Button>
            <AlertDialogAction onClick={onSave}>Save</AlertDialogAction>
          </div>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

export default UnsavedChangesDialog;
