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
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useEnrolWorker } from "@/hooks/api/organizations";
import type { AppAccessSummary } from "@/types/appAccess";
import { apiErrorMessage, apiStatus, pinProblem } from "../utils";
import { AppChecklist } from "./AppChecklist";
import { PinFields } from "./PinFields";

export function EnrolWorkerDialog({
  open,
  onOpenChange,
  orgId,
  apps
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  orgId: string;
  apps: AppAccessSummary[];
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Enrol worker</DialogTitle>
          <DialogDescription>
            They will tap their name on an enrolled kiosk and enter this PIN. Nothing is emailed —
            tell them the PIN yourself.
          </DialogDescription>
        </DialogHeader>
        {/* State lives in the form, which Radix unmounts on close: the PIN is
            gone the moment the dialog is, whichever way it closed. */}
        <EnrolWorkerForm orgId={orgId} apps={apps} onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function EnrolWorkerForm({
  orgId,
  apps,
  onDone
}: {
  orgId: string;
  apps: AppAccessSummary[];
  onDone: () => void;
}) {
  const enrol = useEnrolWorker();
  const [name, setName] = useState("");
  const [identifier, setIdentifier] = useState("");
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [selectedApps, setSelectedApps] = useState<string[]>([]);
  const [identifierError, setIdentifierError] = useState<string | null>(null);
  const [pinError, setPinError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const canSubmit =
    name.trim().length > 0 && identifier.trim().length > 0 && pin.length > 0 && confirm.length > 0;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setIdentifierError(null);
    setFormError(null);
    const problem = pinProblem(pin, confirm);
    setPinError(problem);
    if (problem) return;

    try {
      const created = await enrol.mutateAsync({
        orgId,
        request: { name: name.trim(), identifier: identifier.trim(), pin, apps: selectedApps }
      });
      toast.success(`Enrolled ${created.name}`);
      onDone();
    } catch (err) {
      const status = apiStatus(err);
      if (status === 409) {
        setIdentifierError("Another worker already uses this identifier.");
        return;
      }
      if (status === 403) {
        setFormError(
          "You can't grant apps in this organization. Clear the apps to enrol without any."
        );
        return;
      }
      setFormError(apiErrorMessage(err, "Couldn't enrol the worker"));
    }
  };

  return (
    <form onSubmit={handleSubmit} className='flex flex-col gap-4 pt-1'>
      <div className='space-y-1.5'>
        <Label htmlFor='enrol-name'>Name</Label>
        <Input
          id='enrol-name'
          placeholder='As shown on the kiosk'
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          autoFocus
        />
      </div>
      <div className='space-y-1.5'>
        <Label htmlFor='enrol-identifier'>Identifier</Label>
        <Input
          id='enrol-identifier'
          placeholder='Employee number or short handle'
          autoComplete='off'
          value={identifier}
          onChange={(e) => {
            setIdentifier(e.target.value);
            if (identifierError) setIdentifierError(null);
          }}
          required
          aria-invalid={identifierError ? true : undefined}
          aria-describedby={identifierError ? "enrol-identifier-error" : undefined}
          className={identifierError ? "border-destructive focus-visible:ring-destructive" : ""}
        />
        {identifierError ? (
          <p id='enrol-identifier-error' className='text-destructive text-sm'>
            {identifierError}
          </p>
        ) : (
          <p className='text-muted-foreground text-xs'>Unique within this organization.</p>
        )}
      </div>
      <PinFields
        idPrefix='enrol'
        pin={pin}
        confirm={confirm}
        onPinChange={(v) => {
          setPin(v);
          if (pinError) setPinError(null);
        }}
        onConfirmChange={(v) => {
          setConfirm(v);
          if (pinError) setPinError(null);
        }}
        error={pinError}
      />
      <div className='space-y-1.5'>
        <Label>Apps</Label>
        <AppChecklist
          idPrefix='enrol-app'
          apps={apps}
          selected={selectedApps}
          onChange={setSelectedApps}
        />
      </div>
      {formError && <p className='text-destructive text-sm'>{formError}</p>}
      <div className='flex justify-end gap-2'>
        <Button type='button' variant='outline' size='sm' onClick={onDone}>
          Cancel
        </Button>
        <Button
          type='submit'
          size='sm'
          disabled={!canSubmit || enrol.isPending}
          data-testid='settings-crew-enrol-submit'
        >
          {enrol.isPending ? "Enrolling..." : "Enrol worker"}
        </Button>
      </div>
    </form>
  );
}
