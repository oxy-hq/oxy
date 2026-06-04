import { RefreshCw, Search, ShieldCheck, Users } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent, CardHeader } from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useAdminUsersList } from "@/hooks/api/adminTenants/useAdminUsers";
import ROUTES from "@/libs/utils/routes";
import type { UserStatusId } from "@/services/api/adminTenants";

type StatusFilter = "all" | UserStatusId;

/**
 * `/admin/users` — OXY_OWNER-only directory of every user across every
 * organization. Searchable by email/name, filterable by status.
 */
export default function AdminUsers() {
  const navigate = useNavigate();
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<StatusFilter>("all");

  const {
    data: users = [],
    isLoading,
    isFetching,
    refetch
  } = useAdminUsersList({
    search,
    status: status === "all" ? undefined : status
  });

  return (
    <div className='mx-auto max-w-6xl p-6'>
      <div className='mb-6'>
        <h1 className='font-semibold text-2xl tracking-tight'>Users</h1>
        <p className='mt-1 text-muted-foreground text-sm'>
          Every user across the deployment. Inspect org memberships, manage app-admin role, and
          deactivate accounts.
        </p>
      </div>

      <Card>
        <CardHeader className='flex-row items-center justify-between gap-2 space-y-0 border-b py-4'>
          <div className='flex flex-1 items-center gap-3'>
            <form
              className='relative w-full max-w-sm'
              onSubmit={(e) => {
                e.preventDefault();
                setSearch(searchInput.trim());
              }}
            >
              <Search className='absolute top-1/2 left-2 size-4 -translate-y-1/2 text-muted-foreground' />
              <Input
                value={searchInput}
                onChange={(e) => setSearchInput(e.target.value)}
                placeholder='Search by email or name'
                className='pl-8'
              />
            </form>

            <Select value={status} onValueChange={(v) => setStatus(v as StatusFilter)}>
              <SelectTrigger className='w-36'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='all'>All statuses</SelectItem>
                <SelectItem value='active'>Active</SelectItem>
                <SelectItem value='deleted'>Deactivated</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className='flex items-center gap-3'>
            {!isLoading ? (
              <span className='text-muted-foreground text-sm'>
                {users.length} {users.length === 1 ? "user" : "users"}
              </span>
            ) : null}
            <Button variant='outline' size='sm' onClick={() => refetch()} disabled={isFetching}>
              <RefreshCw className={`size-4 ${isFetching ? "animate-spin" : ""}`} />
              Refresh
            </Button>
          </div>
        </CardHeader>
        <CardContent className='p-0'>
          {isLoading ? (
            <div className='flex items-center justify-center gap-2 py-16 text-muted-foreground text-sm'>
              <Spinner /> Loading…
            </div>
          ) : users.length === 0 ? (
            <EmptyState search={search} />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>User</TableHead>
                  <TableHead className='text-right'>Orgs</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Last login</TableHead>
                  <TableHead>Joined</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.map((u) => (
                  <TableRow
                    key={u.id}
                    className='cursor-pointer hover:bg-muted/40'
                    onClick={() => navigate(ROUTES.ADMIN.USER_DETAIL(u.id))}
                  >
                    <TableCell>
                      <div className='flex items-center gap-3'>
                        <div className='flex size-8 items-center justify-center rounded-full bg-muted font-medium text-muted-foreground text-xs uppercase'>
                          {(u.name || u.email).slice(0, 1)}
                        </div>
                        <div className='flex flex-col'>
                          <span className='flex items-center gap-2 font-medium'>
                            {u.name || u.email}
                            {u.is_app_admin ? (
                              <Badge variant='outline' className='gap-1 px-1.5 py-0 text-[10px]'>
                                <ShieldCheck className='size-3' />
                                Global Admin
                              </Badge>
                            ) : null}
                          </span>
                          <span className='text-muted-foreground text-xs'>{u.email}</span>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className='text-right tabular-nums'>{u.org_count}</TableCell>
                    <TableCell>
                      <StatusBadge status={u.status} />
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm tabular-nums'>
                      {new Date(u.last_login_at).toLocaleDateString()}
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm tabular-nums'>
                      {new Date(u.created_at).toLocaleDateString()}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function StatusBadge({ status }: { status: UserStatusId }) {
  if (status === "deleted") {
    return <Badge variant='outline'>Deactivated</Badge>;
  }
  return (
    <Badge variant='default' className='gap-1.5'>
      <span className='size-1.5 rounded-full bg-current opacity-70' />
      Active
    </Badge>
  );
}

function EmptyState({ search }: { search: string }) {
  return (
    <div className='flex flex-col items-center justify-center gap-2 py-16 text-muted-foreground'>
      <Users className='size-8' />
      <p className='text-sm'>{search ? `No users match "${search}".` : "No users yet."}</p>
      <p className='text-xs'>
        {search
          ? "Try a different search term, or clear the filter."
          : "Users appear here after they sign in for the first time."}
      </p>
    </div>
  );
}
