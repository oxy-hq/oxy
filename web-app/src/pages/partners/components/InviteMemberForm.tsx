import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useInviteMember } from "@/hooks/api/partners";

/** Invite a member to a managed org. Partners can only assign Member/Admin. */
export default function InviteMemberForm({
  partnerId,
  orgId
}: {
  partnerId: string;
  orgId: string;
}) {
  const invite = useInviteMember(partnerId);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState("member");

  const submit = () => {
    const trimmed = email.trim();
    if (!trimmed) return;
    invite.mutate(
      { orgId, email: trimmed, role },
      {
        onSuccess: () => {
          toast.success(`Invited ${trimmed}`);
          setEmail("");
        },
        onError: () => toast.error("Failed to invite member")
      }
    );
  };

  return (
    <div className='flex flex-wrap items-center gap-2'>
      <Input
        type='email'
        placeholder='teammate@company.com'
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        className='max-w-xs'
      />
      <Select value={role} onValueChange={setRole}>
        <SelectTrigger className='w-32'>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value='member'>Member</SelectItem>
          <SelectItem value='admin'>Admin</SelectItem>
        </SelectContent>
      </Select>
      <Button onClick={submit} disabled={invite.isPending || !email.trim()}>
        Invite
      </Button>
    </div>
  );
}
