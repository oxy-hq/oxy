import { ShieldAlert } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useStartAssume } from "@/hooks/api/adminAssume";
import { useActingSession } from "@/hooks/api/adminAssume/useActingSession";

/**
 * Start an assume-role session. The friction here is the feature: an operator
 * must name a reason before stepping into a customer's tenant, and both the
 * reason and the act are written to the tamper-evident audit chain.
 *
 * Without a live session, Oxy staff get NO synthetic Owner membership — opening
 * a tenant org/workspace 403s exactly like it would for any non-member.
 */
export function AssumeRoleDialog({
  open,
  onOpenChange,
  org,
  onStarted
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  org: { id: string; name: string };
  onStarted?: () => void;
}) {
  const [reason, setReason] = useState("");
  const start = useStartAssume();
  // Acting closes whichever console you came from. Naming the wrong one is worse
  // than naming none — a partner has never seen /admin.
  const { isStaff } = useActingSession();

  const submit = () =>
    start.mutate(
      { orgId: org.id, reason: reason.trim() },
      {
        onSuccess: () => {
          setReason("");
          onOpenChange(false);
          onStarted?.();
        }
      }
    );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-md'>
        <DialogHeader>
          <DialogTitle className='flex items-center gap-2'>
            <ShieldAlert className='size-4 text-amber-600 dark:text-amber-400' />
            Act as {org.name}
          </DialogTitle>
          <DialogDescription>
            You'll get Owner-level access to this organization for <b>60 minutes</b>. The session
            can't be extended — start a new one if you need longer. A banner will show while it's
            active, and both the start and the end are written to the audit log under <b>your</b>{" "}
            identity.
          </DialogDescription>
        </DialogHeader>

        <div className='space-y-1.5'>
          <Label htmlFor='assume-reason'>Reason</Label>
          <Input
            id='assume-reason'
            autoFocus
            placeholder='e.g. debugging ticket #482 — their app returns 500'
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && reason.trim()) submit();
            }}
          />
          <p className='text-muted-foreground text-xs'>
            Required. This is what an auditor reads when asking why staff entered this tenant.
          </p>
        </div>

        <div className='space-y-1.5 rounded-md border bg-muted/40 p-2.5 text-muted-foreground text-xs'>
          <p>
            <b>{isStaff ? "Admin closes" : "Your partner console closes"}</b> while you're acting —
            acting as a tenant is a mode, not a badge. You'll come back to it when you stop.
          </p>
          <p>
            Billing, admin-promotion and secrets stay blocked while acting as a tenant — and if this
            org has locked Oxy staff out of its apps, that lockdown still holds.
          </p>
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!reason.trim() || start.isPending} onClick={submit}>
            Start session
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
