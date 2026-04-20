import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useAcceptInvitation } from "@/hooks/api/organizations";
import type { Organization } from "@/types/organization";

export default function JoinOrgDialog({
  open,
  onOpenChange,
  onJoined
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onJoined: (org: Organization) => void;
}) {
  const [token, setToken] = useState("");
  const acceptInvitation = useAcceptInvitation();

  useEffect(() => {
    if (open) setToken("");
  }, [open]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const inviteToken = token.trim();
    if (!inviteToken) return;

    try {
      const org = await acceptInvitation.mutateAsync(inviteToken);
      onJoined(org);
    } catch {
      toast.error("Invalid or expired invitation");
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='font-semibold text-base'>Join organization</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className='flex flex-col gap-4'>
          <div className='space-y-1.5'>
            <Label htmlFor='invite-token'>Invitation code</Label>
            <Input
              id='invite-token'
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder='Paste invitation code or link'
              autoFocus
            />
          </div>
          <Button type='submit' size='sm' disabled={!token.trim() || acceptInvitation.isPending}>
            {acceptInvitation.isPending ? "Joining…" : "Join organization"}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
