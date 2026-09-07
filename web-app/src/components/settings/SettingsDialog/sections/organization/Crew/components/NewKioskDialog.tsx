import { useState } from "react";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useCreateDevice } from "@/hooks/api/organizations";
import type { AppAccessSummary } from "@/types/appAccess";
import type { CreatedKioskDevice } from "@/types/frontline";
import { apiErrorMessage, appReturnTo } from "../utils";

/** Radix Select can't carry an empty value, so "no app" needs a name. App ids are uuids. */
const ORG_HOME = "org-home";

export function NewKioskDialog({
  open,
  onOpenChange,
  orgId,
  orgSlug,
  apps,
  onCreated
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  orgId: string;
  orgSlug: string;
  apps: AppAccessSummary[];
  /** Called with the one-time enrol link; the caller must show it immediately. */
  onCreated: (device: CreatedKioskDevice) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle>New kiosk</DialogTitle>
          <DialogDescription>
            A shared tablet crew sign in on. You'll get a link to open on it once.
          </DialogDescription>
        </DialogHeader>
        <NewKioskForm
          orgId={orgId}
          orgSlug={orgSlug}
          apps={apps}
          onCreated={(device) => {
            onOpenChange(false);
            onCreated(device);
          }}
          onCancel={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}

function NewKioskForm({
  orgId,
  orgSlug,
  apps,
  onCreated,
  onCancel
}: {
  orgId: string;
  orgSlug: string;
  apps: AppAccessSummary[];
  onCreated: (device: CreatedKioskDevice) => void;
  onCancel: () => void;
}) {
  const createDevice = useCreateDevice();
  const [name, setName] = useState("");
  const [appId, setAppId] = useState<string>(ORG_HOME);
  const [error, setError] = useState<string | null>(null);
  const canSubmit = name.trim().length > 0;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setError(null);
    const app = apps.find((a) => a.id === appId);
    try {
      const device = await createDevice.mutateAsync({
        orgId,
        request: {
          name: name.trim(),
          ...(app ? { return_to: appReturnTo(orgSlug, app.slug) } : {})
        }
      });
      onCreated(device);
    } catch (err) {
      setError(apiErrorMessage(err, "Couldn't create the kiosk"));
    }
  };

  return (
    <form onSubmit={handleSubmit} className='flex flex-col gap-4 pt-1'>
      <div className='space-y-1.5'>
        <Label htmlFor='kiosk-name'>Name</Label>
        <Input
          id='kiosk-name'
          placeholder='Front counter'
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          autoFocus
        />
      </div>
      <div className='space-y-1.5'>
        <Label htmlFor='kiosk-opens'>Opens</Label>
        <Select value={appId} onValueChange={setAppId}>
          <SelectTrigger id='kiosk-opens' className='w-full'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ORG_HOME}>Organization home</SelectItem>
            {apps.map((app) => (
              <SelectItem key={app.id} value={app.id}>
                {app.published ? app.name : `${app.name} (not published)`}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className='text-muted-foreground text-xs'>Where the tablet lands after a sign-in.</p>
      </div>
      {error && <p className='text-destructive text-sm'>{error}</p>}
      <div className='flex justify-end gap-2'>
        <Button type='button' variant='outline' size='sm' onClick={onCancel}>
          Cancel
        </Button>
        <Button
          type='submit'
          size='sm'
          disabled={!canSubmit || createDevice.isPending}
          data-testid='settings-crew-new-kiosk-submit'
        >
          {createDevice.isPending ? "Creating..." : "Create kiosk"}
        </Button>
      </div>
    </form>
  );
}
