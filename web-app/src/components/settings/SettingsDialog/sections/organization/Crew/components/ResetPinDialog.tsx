import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { useResetWorkerPin } from "@/hooks/api/organizations";
import type { FrontlineWorker } from "@/types/frontline";
import { apiErrorMessage, pinProblem } from "../utils";
import { PinFields } from "./PinFields";

export function ResetPinDialog({
  open,
  onOpenChange,
  orgId,
  worker
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  orgId: string;
  worker: FrontlineWorker;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle>Reset PIN for {worker.name}</DialogTitle>
          <DialogDescription>
            The old PIN stops working at once, and any lockout is cleared. Tell them the new one
            yourself — nothing is sent.
          </DialogDescription>
        </DialogHeader>
        {/* The PIN lives only in this form, which unmounts with the dialog. */}
        <ResetPinForm orgId={orgId} worker={worker} onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function ResetPinForm({
  orgId,
  worker,
  onDone
}: {
  orgId: string;
  worker: FrontlineWorker;
  onDone: () => void;
}) {
  const resetPin = useResetWorkerPin();
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const canSubmit = pin.length > 0 && confirm.length > 0;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    const problem = pinProblem(pin, confirm);
    setError(problem);
    if (problem) return;
    try {
      await resetPin.mutateAsync({ orgId, userId: worker.user_id, pin });
      toast.success(`PIN reset for ${worker.name}`);
      onDone();
    } catch (err) {
      setError(apiErrorMessage(err, "Couldn't reset the PIN"));
    }
  };

  return (
    <form onSubmit={handleSubmit} className='flex flex-col gap-4 pt-1'>
      <PinFields
        idPrefix='reset'
        pin={pin}
        confirm={confirm}
        onPinChange={(v) => {
          setPin(v);
          if (error) setError(null);
        }}
        onConfirmChange={(v) => {
          setConfirm(v);
          if (error) setError(null);
        }}
        error={error}
        autoFocus
      />
      <div className='flex justify-end gap-2'>
        <Button type='button' variant='outline' size='sm' onClick={onDone}>
          Cancel
        </Button>
        <Button
          type='submit'
          size='sm'
          disabled={!canSubmit || resetPin.isPending}
          data-testid='settings-crew-reset-pin-submit'
        >
          {resetPin.isPending ? "Resetting..." : "Reset PIN"}
        </Button>
      </div>
    </form>
  );
}
