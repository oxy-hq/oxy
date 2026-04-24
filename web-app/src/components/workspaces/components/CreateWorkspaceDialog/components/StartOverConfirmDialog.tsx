import { useState } from "react";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import type { OnboardingResetRequest } from "@/services/api/onboarding";

interface StartOverConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Items that would be wiped if the user confirms. Empty lists are hidden. */
  manifest: OnboardingResetRequest;
  /** Called when the user confirms. The parent owns the actual reset logic. */
  onConfirm: () => Promise<void> | void;
}

const StartOverConfirmDialog = ({
  open,
  onOpenChange,
  manifest,
  onConfirm
}: StartOverConfirmDialogProps) => {
  const [pending, setPending] = useState(false);

  const handleConfirm = async () => {
    setPending(true);
    try {
      // Keep the "Starting over…" state visible for a minimum duration so the
      // action doesn't feel jerky when the reset completes almost instantly.
      const minDuration = new Promise((resolve) => setTimeout(resolve, 600));
      await Promise.all([onConfirm(), minDuration]);
    } finally {
      setPending(false);
    }
  };

  const { secret_names, database_names, model_names, file_paths, directory_paths } = manifest;
  const hasAnyServerSideEffect =
    secret_names.length > 0 ||
    database_names.length > 0 ||
    model_names.length > 0 ||
    file_paths.length > 0 ||
    directory_paths.length > 0;

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Start over?</AlertDialogTitle>
          <AlertDialogDescription>
            This will discard your onboarding progress and permanently remove any files, secrets,
            and database entries that onboarding created. This cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>

        {hasAnyServerSideEffect && (
          <div className='flex max-h-[50vh] flex-col gap-3 overflow-y-auto rounded-md border border-border bg-muted/30 p-3 text-sm'>
            {secret_names.length > 0 && <Section title='Secrets to delete' items={secret_names} />}
            {database_names.length > 0 && (
              <Section title='Database entries to remove from config.yml' items={database_names} />
            )}
            {model_names.length > 0 && (
              <Section title='Model entries to remove from config.yml' items={model_names} />
            )}
            {file_paths.length > 0 && <Section title='Files to delete' items={file_paths} />}
            {directory_paths.length > 0 && (
              <Section title='Directories to delete (recursive)' items={directory_paths} />
            )}
          </div>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel disabled={pending}>Cancel</AlertDialogCancel>
          <Button variant='destructive' onClick={handleConfirm} disabled={pending}>
            {pending ? "Starting over…" : "Start over"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

const Section = ({ title, items }: { title: string; items: string[] }) => (
  <div>
    <div className='font-medium text-foreground text-xs'>{title}</div>
    <ul className='mt-1 ml-4 list-disc text-muted-foreground text-xs'>
      {items.map((item) => (
        <li key={item} className='break-all'>
          {item}
        </li>
      ))}
    </ul>
  </div>
);

export default StartOverConfirmDialog;
