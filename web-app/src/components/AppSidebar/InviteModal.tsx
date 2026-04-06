import { isAxiosError } from "axios";
import { useState } from "react";
import { toast } from "sonner";
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
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useInvite } from "@/hooks/auth/useInvite";

interface InviteModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function InviteModal({ open, onOpenChange }: InviteModalProps) {
  const [email, setEmail] = useState("");
  const { mutate: invite, isPending } = useInvite();

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    invite(
      { email },
      {
        onSuccess: () => {
          toast.success(`Invitation sent to ${email}`);
          setEmail("");
          onOpenChange(false);
        },
        onError: (err) => {
          const message = isAxiosError(err)
            ? (err.response?.data?.message ?? err.message)
            : err.message;
          toast.error(message || "Failed to send invitation");
        }
      }
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Invite a team member</DialogTitle>
          <DialogDescription>
            Send an invitation link to someone so they can sign in to Oxy.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit}>
          <div className='py-4'>
            <Label htmlFor='invite-email'>Email address</Label>
            <Input
              id='invite-email'
              type='email'
              placeholder='colleague@company.com'
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className='mt-2'
              required
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button type='button' variant='outline' onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type='submit' disabled={isPending || !email}>
              {isPending ? <Spinner /> : "Send invitation"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
