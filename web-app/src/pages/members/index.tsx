import { isAxiosError } from "axios";
import { Loader2, MoreHorizontal, Search, ShieldCheck, UserPlus, Users } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { UserAvatar } from "@/components/UserAvatar";
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useAuth } from "@/contexts/AuthContext";
import { useInvite } from "@/hooks/auth/useInvite";
import { useAllUsers, useRemoveUser, useUpdateUserRole } from "@/hooks/auth/useUsers";
import type { UserInfo } from "@/types/auth";

// ─── Permissions ──────────────────────────────────────────────────────────────

type ViewerRole = "owner" | "admin" | "member";

function useCurrentUser(): { user: UserInfo | null; isAdmin: boolean; viewerRole: ViewerRole } {
  const { getUser, authConfig } = useAuth();
  let user: UserInfo | null = null;
  try {
    user = JSON.parse(getUser() || "null");
  } catch {
    // malformed JSON — treat as unauthenticated
  }
  // In single-workspace mode or when auth is disabled, treat all users as owner.
  const noAuth = !authConfig.auth_enabled || !!authConfig.single_workspace;
  const viewerRole: ViewerRole = noAuth
    ? "owner"
    : ((user?.role as ViewerRole | undefined) ?? "member");
  const isAdmin = noAuth || user?.is_admin === true;
  return { user, isAdmin, viewerRole };
}

// ─── Invite dialog ────────────────────────────────────────────────────────────

function InviteDialog({
  open,
  onOpenChange
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
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
              onChange={(e) => setEmail(e.target.value)}
              required
              autoFocus
            />
          </div>
          <div className='flex justify-end gap-2'>
            <Button type='button' variant='outline' size='sm' onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type='submit' size='sm' disabled={isPending || !email}>
              {isPending && <Loader2 className='mr-1.5 h-3.5 w-3.5 animate-spin' />}
              {isPending ? "Sending…" : "Send invite"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ─── Row actions ──────────────────────────────────────────────────────────────

function MemberRowActions({
  user,
  viewerRole,
  isSelf,
  onGrantAdmin,
  onRevokeAdmin,
  onRemove,
  isUpdatingRole,
  isRemoving
}: {
  user: UserInfo;
  viewerRole: ViewerRole;
  isSelf: boolean;
  onGrantAdmin: () => void;
  onRevokeAdmin: () => void;
  onRemove: () => void;
  isUpdatingRole: boolean;
  isRemoving: boolean;
}) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const displayName = user.name || user.email.split("@")[0];
  const targetRole = user.role as ViewerRole;

  // Owners cannot be managed by anyone.
  if (targetRole === "owner" || isSelf) return null;

  // What this viewer can do to this target:
  const canGrantAdmin = viewerRole === "owner" && targetRole === "member";
  const canRevokeAdmin =
    (viewerRole === "owner" || viewerRole === "admin") && targetRole === "admin";
  const canRemove = viewerRole === "owner" || viewerRole === "admin";

  if (!canGrantAdmin && !canRevokeAdmin && !canRemove) return null;

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-8 w-8 data-[state=open]:bg-muted'
            disabled={isUpdatingRole || isRemoving}
          >
            {isUpdatingRole || isRemoving ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <MoreHorizontal className='h-4 w-4' />
            )}
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-44'>
          {canGrantAdmin && <DropdownMenuItem onClick={onGrantAdmin}>Grant admin</DropdownMenuItem>}
          {canRevokeAdmin && (
            <DropdownMenuItem onClick={onRevokeAdmin}>Revoke admin</DropdownMenuItem>
          )}
          {canRemove && (
            <>
              {(canGrantAdmin || canRevokeAdmin) && <DropdownMenuSeparator />}
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
              onClick={onRemove}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function MembersPage() {
  const { user: currentUser, isAdmin, viewerRole } = useCurrentUser();
  const { data, isPending, isError } = useAllUsers();
  const {
    mutate: updateRole,
    variables: updatingRoleVars,
    isPending: isUpdatingRole
  } = useUpdateUserRole();
  const { mutate: removeUser, variables: removingId, isPending: isRemoving } = useRemoveUser();
  const [search, setSearch] = useState("");
  const [inviteOpen, setInviteOpen] = useState(false);

  const users = data?.users ?? [];

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    if (!q) return users;
    return users.filter(
      (u) => u.email.toLowerCase().includes(q) || (u.name || "").toLowerCase().includes(q)
    );
  }, [users, search]);

  const handleSetRole = (user: UserInfo, role: "admin" | "member") => {
    updateRole({ userId: user.id, role }, { onError: () => toast.error("Failed to update role") });
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

  const adminCount = users.filter((u) => u.is_admin).length;

  return (
    <div className='mx-auto w-full max-w-3xl px-6 py-10'>
      {/* Header */}
      <div className='mb-6 flex items-end justify-between'>
        <div>
          <h2 className='font-bold text-2xl tracking-tight'>Members</h2>
          <p className='text-muted-foreground text-sm'>
            {users.length} {users.length === 1 ? "member" : "members"} · {adminCount}{" "}
            {adminCount === 1 ? "admin" : "admins"}
          </p>
        </div>
        {isAdmin && (
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
          placeholder='Search members…'
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className='pl-9'
        />
      </div>

      {/* Table */}
      {filtered.length === 0 ? (
        <div className='flex flex-col items-center gap-3 rounded-md border py-20 text-center'>
          <Users className='h-8 w-8 text-muted-foreground/30' />
          <p className='text-muted-foreground text-sm'>
            {search ? `No members match "${search}"` : "No members yet"}
          </p>
          {!search && isAdmin && (
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
              {filtered.map((user) => {
                const isSelf = user.id === currentUser?.id;
                const displayName = user.name || user.email.split("@")[0];

                return (
                  <TableRow key={user.id}>
                    {/* Identity */}
                    <TableCell className='px-4 py-3'>
                      <div className='flex items-center gap-3'>
                        <UserAvatar
                          name={user.name ?? ""}
                          email={user.email}
                          picture={user.picture}
                          className='size-8 rounded-md'
                        />
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
                            {user.email}
                          </span>
                        </div>
                      </div>
                    </TableCell>

                    {/* Role */}
                    <TableCell className='w-32 px-4 py-3'>
                      {user.role === "owner" ? (
                        <Badge
                          variant='outline'
                          className='gap-1 border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
                        >
                          <ShieldCheck className='h-3 w-3' />
                          Owner
                        </Badge>
                      ) : user.is_admin ? (
                        <Badge
                          variant='outline'
                          className='gap-1 border-primary/30 bg-primary/5 text-primary'
                        >
                          <ShieldCheck className='h-3 w-3' />
                          Admin
                        </Badge>
                      ) : (
                        <Badge variant='outline' className='text-muted-foreground'>
                          Member
                        </Badge>
                      )}
                    </TableCell>

                    {/* Actions */}
                    <TableCell className='w-12 px-2 py-3 text-right'>
                      <MemberRowActions
                        user={user}
                        viewerRole={viewerRole}
                        isSelf={isSelf}
                        onGrantAdmin={() => handleSetRole(user, "admin")}
                        onRevokeAdmin={() => handleSetRole(user, "member")}
                        onRemove={() => removeUser(user.id)}
                        isUpdatingRole={isUpdatingRole && updatingRoleVars?.userId === user.id}
                        isRemoving={isRemoving && removingId === user.id}
                      />
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </div>
      )}

      <InviteDialog open={inviteOpen} onOpenChange={setInviteOpen} />
    </div>
  );
}
