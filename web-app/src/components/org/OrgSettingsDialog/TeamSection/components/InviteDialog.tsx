import { isAxiosError } from "axios";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useCreateInvitation } from "@/hooks/api/organizations";
import type { OrgRole } from "@/types/organization";

export function InviteDialog({
  open,
  onOpenChange,
  orgId,
  viewerRole
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  orgId: string;
  viewerRole: OrgRole;
}) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<OrgRole>("member");
  const [emailError, setEmailError] = useState<string | null>(null);
  const createInvitation = useCreateInvitation();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email.trim()) return;
    setEmailError(null);
    try {
      await createInvitation.mutateAsync({ orgId, email: email.trim(), role });
      toast.success(`Invitation sent to ${email}`);
      setEmail("");
      setRole("member");
      onOpenChange(false);
    } catch (err) {
      if (isAxiosError(err) && err.response?.status === 409) {
        setEmailError("This email is already a member or has a pending invitation.");
        return;
      }
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to send invitation";
      setEmailError(message);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle>Invite member</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className='flex flex-col gap-4 pt-1'>
          <div className='space-y-1.5'>
            <Label htmlFor='invite-email'>Email address</Label>
            <Input
              id='invite-email'
              type='email'
              placeholder='colleague@company.com'
              value={email}
              onChange={(e) => {
                setEmail(e.target.value);
                if (emailError) setEmailError(null);
              }}
              required
              autoFocus
              aria-invalid={emailError ? true : undefined}
              aria-describedby={emailError ? "invite-email-error" : undefined}
              className={emailError ? "border-destructive focus-visible:ring-destructive" : ""}
            />
            {emailError && (
              <p id='invite-email-error' className='text-destructive text-sm'>
                {emailError}
              </p>
            )}
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='invite-role'>Role</Label>
            <Select value={role} onValueChange={(v) => setRole(v as OrgRole)}>
              <SelectTrigger id='invite-role'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {viewerRole === "owner" && <SelectItem value='owner'>Owner</SelectItem>}
                <SelectItem value='admin'>Admin</SelectItem>
                <SelectItem value='member'>Member</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className='flex justify-end gap-2'>
            <Button type='button' variant='outline' size='sm' onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type='submit' size='sm' disabled={!email.trim() || createInvitation.isPending}>
              {createInvitation.isPending ? "Sending..." : "Send invite"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
