import { Copy, Loader2, Search, Trash2, UserPlus, Users } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useOrgInvitations, useOrgMembers, useRevokeInvitation } from "@/hooks/api/organizations";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import type { Organization, OrgRole } from "@/types/organization";
import { InviteDialog } from "./components/InviteDialog";
import { MemberRowActions } from "./components/MemberRowActions";
import { RoleBadge } from "./components/RoleBadge";

interface TeamSectionProps {
  org: Organization;
  viewerRole: OrgRole;
}

export default function TeamSection({ org, viewerRole }: TeamSectionProps) {
  const orgId = org.id;
  const { data: currentUser } = useCurrentUser();
  const { data: members, isPending, isError } = useOrgMembers(orgId);
  const { data: invitations } = useOrgInvitations(orgId);
  const revokeInvitation = useRevokeInvitation();
  const [search, setSearch] = useState("");
  const [inviteOpen, setInviteOpen] = useState(false);

  const canManage = viewerRole === "owner" || viewerRole === "admin";
  const ownerCount = members?.filter((m) => m.role === "owner").length ?? 0;
  const adminCount = members?.filter((m) => m.role === "admin").length ?? 0;
  const pendingInvitations = invitations?.filter((inv) => inv.status === "pending") ?? [];

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    if (!q || !members) return members ?? [];
    return members.filter(
      (m) => m.email.toLowerCase().includes(q) || (m.name || "").toLowerCase().includes(q)
    );
  }, [members, search]);

  const handleRevoke = async (invitationId: string) => {
    try {
      await revokeInvitation.mutateAsync({ orgId, invitationId });
      toast.success("Invitation revoked");
    } catch {
      toast.error("Failed to revoke invitation");
    }
  };

  if (isPending) {
    return (
      <div className='flex min-h-40 w-full items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex min-h-40 w-full items-center justify-center'>
        <p className='text-destructive text-sm'>Failed to load members.</p>
      </div>
    );
  }

  return (
    <div className='space-y-6'>
      <div className='flex items-end justify-between gap-4'>
        <div>
          <h3 className='font-medium'>Members</h3>
          <p className='text-muted-foreground text-sm'>
            {members?.length ?? 0} {(members?.length ?? 0) === 1 ? "member" : "members"} ·{" "}
            {adminCount} {adminCount === 1 ? "admin" : "admins"}
          </p>
        </div>
        {canManage && (
          <Button onClick={() => setInviteOpen(true)} size='sm' className='gap-1.5'>
            <UserPlus className='h-4 w-4' />
            Invite member
          </Button>
        )}
      </div>

      <div className='relative'>
        <Search className='absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground' />
        <Input
          placeholder='Search members...'
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className='pl-9'
        />
      </div>

      {filtered.length === 0 ? (
        <div className='flex flex-col items-center gap-3 rounded-md border py-12 text-center'>
          <Users className='h-8 w-8 text-muted-foreground/30' />
          <p className='text-muted-foreground text-sm'>
            {search ? `No members match "${search}"` : "No members yet"}
          </p>
          {!search && canManage && (
            <Button
              size='sm'
              variant='outline'
              className='mt-1 gap-1.5'
              onClick={() => setInviteOpen(true)}
            >
              <UserPlus className='h-4 w-4' />
              Invite your first member
            </Button>
          )}
        </div>
      ) : (
        <div className='overflow-hidden rounded-md border'>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className='px-4'>Name</TableHead>
                <TableHead className='w-32 px-4'>Role</TableHead>
                <TableHead className='w-12' />
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((member) => {
                const isSelf = member.user_id === currentUser?.id;
                const displayName = member.name || member.email.split("@")[0];

                return (
                  <TableRow key={member.id}>
                    <TableCell className='px-4 py-3'>
                      <div className='flex items-center gap-3'>
                        <div className='flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted font-medium text-sm'>
                          {member.name?.[0]?.toUpperCase() ?? "?"}
                        </div>
                        <div className='flex flex-col gap-0.5'>
                          <span className='font-medium text-sm leading-none'>
                            {displayName}
                            {isSelf && (
                              <span className='ml-1.5 text-[11px] text-muted-foreground'>
                                (you)
                              </span>
                            )}
                          </span>
                          <span className='font-mono text-muted-foreground text-xs'>
                            {member.email}
                          </span>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className='w-32 px-4 py-3'>
                      <RoleBadge role={member.role} />
                    </TableCell>
                    <TableCell className='w-12 px-2 py-3 text-right'>
                      <MemberRowActions
                        member={member}
                        orgId={orgId}
                        viewerRole={viewerRole}
                        isSelf={isSelf}
                        ownerCount={ownerCount}
                      />
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </div>
      )}

      {canManage && pendingInvitations.length > 0 && (
        <div className='space-y-3'>
          <h3 className='font-medium'>Pending Invitations</h3>
          <div className='divide-y divide-border rounded-lg border border-border'>
            {pendingInvitations.map((inv) => (
              <div key={inv.id} className='flex items-center gap-3 px-4 py-3'>
                <div className='min-w-0 flex-1'>
                  <div className='truncate text-sm'>{inv.email}</div>
                  <div className='text-muted-foreground text-xs capitalize'>{inv.role}</div>
                </div>
                <div className='text-muted-foreground text-xs'>
                  Expires {new Date(inv.expires_at).toLocaleDateString()}
                </div>
                <Button
                  variant='ghost'
                  size='icon'
                  onClick={async () => {
                    const inviteUrl = `${window.location.origin}/invite/${inv.token}`;
                    try {
                      await navigator.clipboard.writeText(inviteUrl);
                      toast.success("Invite link copied");
                    } catch {
                      toast.error("Failed to copy invite link");
                    }
                  }}
                  title='Copy invite link'
                >
                  <Copy className='h-4 w-4 text-muted-foreground' />
                </Button>
                <Button variant='ghost' size='icon' onClick={() => handleRevoke(inv.id)}>
                  <Trash2 className='h-4 w-4 text-muted-foreground' />
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}

      <InviteDialog
        open={inviteOpen}
        onOpenChange={setInviteOpen}
        orgId={orgId}
        viewerRole={viewerRole}
      />
    </div>
  );
}
