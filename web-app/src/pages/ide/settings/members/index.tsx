import { isAxiosError } from "axios";
import { Loader2, RotateCcw, Search, ShieldCheck, Users } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
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
  useRemoveWorkspaceRoleOverride,
  useSetWorkspaceRoleOverride,
  useWorkspaceMembers
} from "@/hooks/api/workspaces";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import PageHeader from "@/pages/ide/components/PageHeader";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { OrgRole, WorkspaceMember, WorkspaceRole } from "@/types/organization";

// ─── Role badges ─────────────────────────────────────────────────────────────

function OrgRoleBadge({ role }: { role: OrgRole }) {
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

function WorkspaceRoleBadge({ role }: { role: WorkspaceRole }) {
  const label = role.charAt(0).toUpperCase() + role.slice(1);

  if (role === "owner") {
    return (
      <Badge
        variant='outline'
        className='gap-1 border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
      >
        {label}
      </Badge>
    );
  }
  if (role === "admin") {
    return (
      <Badge variant='outline' className='gap-1 border-primary/30 bg-primary/5 text-primary'>
        {label}
      </Badge>
    );
  }
  if (role === "viewer") {
    return (
      <Badge variant='outline' className='text-muted-foreground/70'>
        {label}
      </Badge>
    );
  }
  return (
    <Badge variant='outline' className='text-muted-foreground'>
      {label}
    </Badge>
  );
}

// ─── Role selector ───────────────────────────────────────────────────────────

function RoleSelector({
  member,
  workspaceId,
  canManage
}: {
  member: WorkspaceMember;
  workspaceId: string;
  canManage: boolean;
}) {
  const setOverride = useSetWorkspaceRoleOverride();
  const removeOverride = useRemoveWorkspaceRoleOverride();

  const handleRoleChange = (newRole: string) => {
    if (newRole === member.workspace_role) return;
    setOverride.mutate(
      { workspaceId, userId: member.user_id, role: newRole },
      {
        onSuccess: () => toast.success(`Role updated to ${newRole}`),
        onError: (err) => {
          const message = isAxiosError(err)
            ? (err.response?.data?.message ?? err.message)
            : err instanceof Error
              ? err.message
              : "Failed to update role";
          toast.error(message);
        }
      }
    );
  };

  const handleResetOverride = () => {
    removeOverride.mutate(
      { workspaceId, userId: member.user_id },
      {
        onSuccess: () => toast.success("Role override removed"),
        onError: (err) => {
          const message = isAxiosError(err)
            ? (err.response?.data?.message ?? err.message)
            : err instanceof Error
              ? err.message
              : "Failed to remove override";
          toast.error(message);
        }
      }
    );
  };

  const isPending = setOverride.isPending || removeOverride.isPending;

  if (!canManage) {
    return <WorkspaceRoleBadge role={member.workspace_role} />;
  }

  return (
    <div className='flex items-center gap-1.5'>
      <Select value={member.workspace_role} onValueChange={handleRoleChange} disabled={isPending}>
        <SelectTrigger className='h-8 w-28'>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value='owner'>Owner</SelectItem>
          <SelectItem value='admin'>Admin</SelectItem>
          <SelectItem value='member'>Member</SelectItem>
          <SelectItem value='viewer'>Viewer</SelectItem>
        </SelectContent>
      </Select>
      {member.is_override && (
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          onClick={handleResetOverride}
          disabled={isPending}
          tooltip={{ content: "Reset to org role", side: "right" }}
        >
          {isPending ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : (
            <RotateCcw className='h-3.5 w-3.5 text-muted-foreground' />
          )}
        </Button>
      )}
    </div>
  );
}

// ─── Page ────────────────────────────────────────────────────────────────────

export default function WorkspaceMembersPage() {
  const { project } = useCurrentProjectBranch();
  const workspaceId = project.id;
  const orgRole = useCurrentOrg((s) => s.role) ?? "member";
  const canManage = orgRole === "owner" || orgRole === "admin";

  const { data: members, isPending, isError } = useWorkspaceMembers(workspaceId);
  const [search, setSearch] = useState("");

  const overrideCount = useMemo(() => members?.filter((m) => m.is_override).length ?? 0, [members]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    if (!q || !members) return members ?? [];
    return members.filter(
      (m) => m.email.toLowerCase().includes(q) || (m.name || "").toLowerCase().includes(q)
    );
  }, [members, search]);

  if (isPending) {
    return (
      <div className='flex h-full flex-col'>
        <PageHeader icon={Users} title='Workspace Members' />
        <div className='flex flex-1 items-center justify-center'>
          <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
        </div>
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full flex-col'>
        <PageHeader icon={Users} title='Workspace Members' />
        <div className='flex flex-1 items-center justify-center'>
          <p className='text-destructive text-sm'>Failed to load members.</p>
        </div>
      </div>
    );
  }

  const subtitle = [
    `${members?.length ?? 0} ${(members?.length ?? 0) === 1 ? "member" : "members"}`,
    overrideCount > 0 ? `${overrideCount} ${overrideCount === 1 ? "override" : "overrides"}` : null
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className='flex h-full flex-col'>
      <PageHeader
        icon={Users}
        title={
          <span className='flex items-center gap-2'>
            Workspace Members
            <span className='font-normal text-muted-foreground text-xs'>{subtitle}</span>
          </span>
        }
      />

      <div className='scrollbar-gutter-auto min-h-0 flex-1 overflow-auto p-4'>
        {/* Search */}
        <div className='relative mb-4 max-w-sm'>
          <Search className='absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground' />
          <Input
            placeholder='Search members...'
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
          </div>
        ) : (
          <div className='overflow-hidden rounded-md border'>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className='px-4'>Name</TableHead>
                  <TableHead className='w-32 px-4'>Org Role</TableHead>
                  <TableHead className='w-40 px-4'>Workspace Role</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filtered.map((member) => {
                  const displayName = member.name || member.email.split("@")[0];

                  return (
                    <TableRow key={member.user_id}>
                      <TableCell className='px-4 py-3'>
                        <div className='flex items-center gap-3'>
                          <div className='flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted font-medium text-sm'>
                            {member.name?.[0]?.toUpperCase() ?? "?"}
                          </div>
                          <div className='flex flex-col gap-0.5'>
                            <span className='font-medium text-sm leading-none'>{displayName}</span>
                            <span className='font-mono text-muted-foreground text-xs'>
                              {member.email}
                            </span>
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className='w-32 px-4 py-3'>
                        <OrgRoleBadge role={member.org_role} />
                      </TableCell>
                      <TableCell className='w-40 px-4 py-3'>
                        <RoleSelector
                          member={member}
                          workspaceId={workspaceId}
                          canManage={canManage}
                        />
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>
        )}
      </div>
    </div>
  );
}
