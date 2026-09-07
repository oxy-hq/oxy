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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";
import { useCreateRole } from "@/hooks/api/organizations";
import { apiErrorMessage, apiStatus } from "@/libs/apiError";
import { SCOPE_LABELS } from "@/libs/operatingGraph";
import type { RoleScope } from "@/types/operatingGraph";

export function NewPositionDialog({
  open,
  onOpenChange,
  orgId
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  orgId: string;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle>New position</DialogTitle>
          <DialogDescription>
            A position is what someone is called and what work routes to them. It grants no
            permissions.
          </DialogDescription>
        </DialogHeader>
        <NewPositionForm orgId={orgId} onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function NewPositionForm({ orgId, onDone }: { orgId: string; onDone: () => void }) {
  const create = useCreateRole();
  const [name, setName] = useState("");
  const [scope, setScope] = useState<RoleScope>("location");
  const [error, setError] = useState<string | null>(null);
  const canSubmit = name.trim().length > 0;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit || create.isPending) return;
    setError(null);
    try {
      const created = await create.mutateAsync({ orgId, request: { name: name.trim(), scope } });
      toast.success(`Created ${created.name}`);
      onDone();
    } catch (err) {
      if (apiStatus(err) === 409) {
        setError("A position with that name already exists. Pick another name.");
        return;
      }
      setError(apiErrorMessage(err, "Couldn't create the position"));
    }
  };

  return (
    <form onSubmit={handleSubmit} className='flex flex-col gap-4 pt-1'>
      <div className='space-y-1.5'>
        <Label htmlFor='position-name'>Name</Label>
        <Input
          id='position-name'
          placeholder='Shift lead'
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          autoFocus
          data-testid='settings-positions-name'
        />
      </div>
      <div className='space-y-1.5'>
        <Label id='position-scope-label'>Scope</Label>
        <ToggleGroup
          type='single'
          variant='outline'
          value={scope}
          onValueChange={(v) => {
            if (v) setScope(v as RoleScope);
          }}
          aria-labelledby='position-scope-label'
          className='justify-start'
          data-testid='settings-positions-scope'
        >
          <ToggleGroupItem value='location' data-testid='settings-positions-scope-location'>
            {SCOPE_LABELS.location}
          </ToggleGroupItem>
          <ToggleGroupItem value='franchisor' data-testid='settings-positions-scope-franchisor'>
            {SCOPE_LABELS.franchisor}
          </ToggleGroupItem>
        </ToggleGroup>
        <p className='text-muted-foreground text-xs'>
          {scope === "location"
            ? "Held at one place at a time, like a store manager or a shift lead."
            : "Held across the whole organization, like an area manager."}
        </p>
      </div>
      {error && <p className='text-destructive text-sm'>{error}</p>}
      <div className='flex justify-end gap-2'>
        <Button type='button' variant='outline' size='sm' onClick={onDone}>
          Cancel
        </Button>
        <Button
          type='submit'
          size='sm'
          disabled={!canSubmit || create.isPending}
          data-testid='settings-positions-submit'
        >
          {create.isPending ? "Creating..." : "Create position"}
        </Button>
      </div>
    </form>
  );
}
