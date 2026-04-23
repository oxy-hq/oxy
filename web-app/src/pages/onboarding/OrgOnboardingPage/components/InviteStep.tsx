import { isAxiosError } from "axios";
import { ArrowRight, Loader2, Mail, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { CanOrgAdmin } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useCreateBulkInvitations } from "@/hooks/api/organizations";
import type { OrgRole } from "@/types/organization";

type PendingInvite = { email: string; role: OrgRole };

// Pragmatic client-side check (matches the spec used by `<input type=email>`):
// rejects obvious garbage like `@`, `a@`, `foo@bar`. Authoritative validation
// lives on the backend (`email_address` crate) — this only spares the user a
// round-trip for the trivial cases.
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

// Mirrors the backend `MAX_BULK_INVITES` cap so the user gets immediate
// feedback instead of filling in 60 addresses and hitting a 422 at submit.
const MAX_INVITES = 50;

/** Inline invite form for the org-onboarding wizard. Fire-and-collect: queues
 *  invites locally, ships them as a single bulk request on Continue. The
 *  backend wraps the batch in a transaction — all succeed or none do — so
 *  there's no partial-state to reconcile here. Skip jumps straight to the
 *  workspace step without sending anything. */
export default function InviteStep({
  orgId,
  orgName,
  onContinue
}: {
  orgId: string;
  orgName: string;
  onContinue: () => void;
}) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<OrgRole>("member");
  const [invites, setInvites] = useState<PendingInvite[]>([]);
  const [emailError, setEmailError] = useState<string | null>(null);
  const createBulkInvitations = useCreateBulkInvitations();

  const atLimit = invites.length >= MAX_INVITES;

  const handleAdd = () => {
    const trimmed = email.trim();
    if (!trimmed) return;
    if (atLimit) {
      setEmailError(`You can invite up to ${MAX_INVITES} people at a time.`);
      return;
    }
    if (!EMAIL_RE.test(trimmed)) {
      setEmailError("Enter a valid email address");
      return;
    }
    if (invites.some((i) => i.email.toLowerCase() === trimmed.toLowerCase())) {
      setEmailError("This email is already in the list");
      return;
    }
    setInvites([...invites, { email: trimmed, role }]);
    setEmail("");
    setEmailError(null);
  };

  const handleRemove = (target: string) => {
    setInvites(invites.filter((i) => i.email !== target));
  };

  const handleSendAndContinue = async () => {
    if (invites.length === 0) {
      onContinue();
      return;
    }

    try {
      const sent = await createBulkInvitations.mutateAsync({ orgId, invitations: invites });
      toast.success(`Sent ${sent.length} invitation${sent.length === 1 ? "" : "s"}`);
      onContinue();
    } catch (err) {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : "Failed to send invitations";
      toast.error(message);
    }
  };

  return (
    <CanOrgAdmin
      fallback={
        <Card>
          <CardHeader>
            <CardTitle className='text-lg'>Invite your team to {orgName}</CardTitle>
            <CardDescription>
              Only org owners and admins can invite members. Ask an admin to continue — or skip this
              step.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className='flex justify-end pt-2'>
              <Button onClick={onContinue}>
                Skip
                <ArrowRight className='size-3.5' />
              </Button>
            </div>
          </CardContent>
        </Card>
      }
    >
      <Card>
        <CardHeader>
          <CardTitle className='text-lg'>Invite your team to {orgName}</CardTitle>
          <CardDescription>Add teammates now or skip and invite them later.</CardDescription>
        </CardHeader>
        <CardContent className='flex flex-col gap-4'>
          <div className='flex flex-col gap-1'>
            <div className='flex gap-2'>
              <div className='flex-1 space-y-1.5'>
                <Label htmlFor='invite-email' className='sr-only'>
                  Email
                </Label>
                <Input
                  id='invite-email'
                  type='email'
                  placeholder='colleague@company.com'
                  value={email}
                  onChange={(e) => {
                    setEmail(e.target.value);
                    if (emailError) setEmailError(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleAdd();
                    }
                  }}
                  aria-invalid={emailError ? true : undefined}
                  className={emailError ? "border-destructive focus-visible:ring-destructive" : ""}
                />
              </div>
              <Select value={role} onValueChange={(v) => setRole(v as OrgRole)}>
                <SelectTrigger className='w-32'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value='admin'>Admin</SelectItem>
                  <SelectItem value='member'>Member</SelectItem>
                </SelectContent>
              </Select>
              <Button
                type='button'
                variant='outline'
                onClick={handleAdd}
                disabled={!email.trim() || atLimit}
              >
                Add
              </Button>
            </div>
            {emailError && <p className='text-destructive text-sm'>{emailError}</p>}
            {!emailError && atLimit && (
              <p className='text-muted-foreground text-xs'>
                You can invite up to {MAX_INVITES} people at once. Remove one to add more.
              </p>
            )}
          </div>
          {invites.length > 0 && (
            <div className='flex flex-col gap-2 rounded-md border p-3'>
              {invites.map((inv) => (
                <div key={inv.email} className='flex items-center gap-2 text-sm'>
                  <Mail className='size-3.5 text-muted-foreground' />
                  <span className='flex-1 truncate'>{inv.email}</span>
                  <span className='text-muted-foreground text-xs capitalize'>{inv.role}</span>
                  <Button
                    type='button'
                    variant='ghost'
                    size='icon'
                    className='size-6'
                    onClick={() => handleRemove(inv.email)}
                  >
                    <X className='size-3.5' />
                  </Button>
                </div>
              ))}
            </div>
          )}

          <div className='flex items-center justify-end gap-2 pt-2'>
            <Button variant='ghost' onClick={onContinue} disabled={createBulkInvitations.isPending}>
              Skip for now
            </Button>
            <Button onClick={handleSendAndContinue} disabled={createBulkInvitations.isPending}>
              {createBulkInvitations.isPending ? (
                <Loader2 className='size-4 animate-spin' />
              ) : (
                <>
                  {invites.length > 0 ? "Send & continue" : "Continue"}
                  <ArrowRight className='size-3.5' />
                </>
              )}
            </Button>
          </div>
        </CardContent>
      </Card>
    </CanOrgAdmin>
  );
}
