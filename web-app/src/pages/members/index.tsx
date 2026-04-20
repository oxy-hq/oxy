import { isAxiosError } from "axios";
import {
  Copy,
  Loader2,
  MoreHorizontal,
  Search,
  ShieldCheck,
  Trash2,
  UserPlus,
  Users
} from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import {
  useCreateInvitation,
  useOrgInvitations,
  useOrgMembers,
  useRemoveMember,
  useRevokeInvitation,
  useUpdateMemberRole
} from "@/hooks/api/organizations";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { OrgMember, OrgRole } from "@/types/organization";

// ─── Invite dialog ────────────────────────────────────────────────────────────

function InviteDialog({
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

// ─── Row actions ──────────────────────────────────────────────────────────────

function MemberRowActions({
  member,
  orgId,
  viewerRole,
  isSelf,
  ownerCount
}: {
  member: OrgMember;
  orgId: string;
  viewerRole: OrgRole;
  isSelf: boolean;
  ownerCount: number;
}) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const updateRole = useUpdateMemberRole();
  const removeMember = useRemoveMember();
  const displayName = member.name || member.email.split("@")[0];
  const isLastOwner = member.role === "owner" && ownerCount <= 1;

  if (isSelf || isLastOwner) return null;

  const canManage = viewerRole === "owner" || viewerRole === "admin";
  if (!canManage) return null;

  const canGrantAdmin = viewerRole === "owner" && member.role === "member";
  const canGrantOwner = viewerRole === "owner" && member.role !== "owner";
  const canRevokeAdmin =
    (viewerRole === "owner" || viewerRole === "admin") && member.role === "admin";
  const canDemoteToMember = viewerRole === "owner" && member.role !== "member";
  const canRemove = true;

  const handleRoleChange = (role: string) => {
    updateRole.mutate(
      { orgId, userId: member.user_id, role },
      { onError: () => toast.error("Failed to update role") }
    );
  };

  const handleRemove = () => {
    removeMember.mutate(
      { orgId, userId: member.user_id },
      {
        onSuccess: () => toast.success(`${displayName} removed`),
        onError: () => toast.error("Failed to remove member")
      }
    );
  };

  const isPending = updateRole.isPending || removeMember.isPending;

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-8 w-8 data-[state=open]:bg-muted'
            disabled={isPending}
          >
            {isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <MoreHorizontal className='h-4 w-4' />
            )}
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-44'>
          {canGrantOwner && member.role !== "owner" && (
            <DropdownMenuItem onClick={() => handleRoleChange("owner")}>
              Make owner
            </DropdownMenuItem>
          )}
          {canGrantAdmin && (
            <DropdownMenuItem onClick={() => handleRoleChange("admin")}>
              Grant admin
            </DropdownMenuItem>
          )}
          {canRevokeAdmin && (
            <DropdownMenuItem onClick={() => handleRoleChange("member")}>
              Revoke admin
            </DropdownMenuItem>
          )}
          {canDemoteToMember && member.role !== "member" && !canRevokeAdmin && (
            <DropdownMenuItem onClick={() => handleRoleChange("member")}>
              Demote to member
            </DropdownMenuItem>
          )}
          {canRemove && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className='text-destructive focus:text-destructive'
                onClick={() => setDeleteOpen(true)}
              >
                Remove
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove {displayName}?</AlertDialogTitle>
            <AlertDialogDescription>
              This will revoke their access. They can be re-invited later.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
              onClick={handleRemove}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

// ─── Role badge ───────────────────────────────────────────────────────────────

function RoleBadge({ role }: { role: OrgRole }) {
  if (role === "owner") {
    return (
      <Badge
        variant='outline'
        className='gap-1 border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
      >
        <ShieldCheck className='h-3 w-3' />
        Owner
      </Badge>
    );
  }
  if (role === "admin") {
    return (
      <Badge variant='outline' className='gap-1 border-primary/30 bg-primary/5 text-primary'>
        <ShieldCheck className='h-3 w-3' />
        Admin
      </Badge>
    );
  }
  return (
    <Badge variant='outline' className='text-muted-foreground'>
      Member
    </Badge>
  );
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function MembersPage() {
  const org = useCurrentOrg((s) => s.org);
  const viewerRole = useCurrentOrg((s) => s.role) ?? "member";
  const orgId = org?.id ?? "";
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
      <div className='flex h-full w-full items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-destructive text-sm'>Failed to load members.</p>
      </div>
    );
  }

  return (
    <div className='mx-auto w-full max-w-3xl px-6 py-10'>
      {/* Header */}
      <div className='mb-6 flex items-end justify-between'>
        <div>
          <h2 className='font-bold text-2xl tracking-tight'>Members</h2>
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

      {/* Search */}
      <div className='relative mb-4'>
        <Search className='absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground' />
        <Input
          placeholder='Search members...'
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className='pl-9'
        />
      </div>

      {/* Members table */}
      {filtered.length === 0 ? (
        <div className='flex flex-col items-center gap-3 rounded-md border py-20 text-center'>
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

      {/* Pending invitations */}
      {canManage && pendingInvitations.length > 0 && (
        <div className='mt-8 space-y-3'>
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
