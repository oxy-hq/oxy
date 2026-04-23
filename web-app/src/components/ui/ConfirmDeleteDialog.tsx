import { CircleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  confirmationName: string;
  confirmButtonLabel: string;
  onConfirm: () => void;
  isPending?: boolean;
};

export function ConfirmDeleteDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmationName,
  confirmButtonLabel,
  onConfirm,
  isPending = false
}: Props) {
  const [typed, setTyped] = useState("");

  useEffect(() => {
    if (!open) setTyped("");
  }, [open]);

  const matches = typed.trim() === confirmationName.trim();

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent className='gap-6 sm:max-w-md'>
        <div className='flex flex-col items-center gap-3 text-center'>
          <CircleAlert className='size-10 text-destructive' />
          <AlertDialogTitle className='text-center'>{title}</AlertDialogTitle>
          <AlertDialogDescription className='text-center'>{description}</AlertDialogDescription>
        </div>

        <Input
          autoFocus
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          placeholder={confirmationName}
          aria-label={`Type "${confirmationName}" to confirm`}
        />

        <div className='flex flex-col items-center gap-3'>
          <Button
            variant='destructive'
            className='w-full'
            disabled={!matches || isPending}
            onClick={onConfirm}
          >
            {isPending ? "Deleting..." : confirmButtonLabel}
          </Button>
          <button
            type='button'
            onClick={() => onOpenChange(false)}
            disabled={isPending}
            className='text-muted-foreground text-sm hover:text-foreground disabled:opacity-50'
          >
            Cancel
          </button>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  );
}
