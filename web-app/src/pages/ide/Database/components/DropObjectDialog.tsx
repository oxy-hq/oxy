import type React from "react";
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

interface DropObjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  kind: "schema" | "table";
  /** Human-readable name shown in the prompt. */
  name: string;
  onConfirm: () => void;
  isPending: boolean;
}

/** Destructive-confirm dialog for dropping a schema or table from the Database
 *  sidebar. Mirrors the Files sidebar's delete confirm. */
const DropObjectDialog: React.FC<DropObjectDialogProps> = ({
  open,
  onOpenChange,
  kind,
  name,
  onConfirm,
  isPending
}) => (
  <AlertDialog open={open} onOpenChange={onOpenChange}>
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>
          Drop {kind} &ldquo;{name}&rdquo;?
        </AlertDialogTitle>
        <AlertDialogDescription>
          {kind === "schema"
            ? "This permanently drops the schema and every table inside it (CASCADE). This cannot be undone."
            : "This permanently drops the table and its data. This cannot be undone."}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
        <AlertDialogAction
          // Keep the dialog open while the drop runs; the caller closes it on success.
          onClick={(e) => {
            e.preventDefault();
            onConfirm();
          }}
          disabled={isPending}
          className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
        >
          {isPending ? "Dropping…" : `Drop ${kind}`}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
);

export default DropObjectDialog;
