import { RefreshCw, Search, Users } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
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
import { Table, TableBody, TableCell, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import { useAdminUsersList } from "@/hooks/api/adminTenants/useAdminUsers";
import ROUTES from "@/libs/utils/routes";
import type { UserStatusId } from "@/services/api/adminTenants";
import { AdminStatusPill } from "../components/AdminStatusPill";
import { ADMIN_HEADER_ROW_CLASS, ADMIN_ROW_CLASS, AdminTh } from "../components/AdminTable";
import { orgRoleKind, RoleBadge } from "../components/RoleBadge";

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
                <TableRow className={ADMIN_HEADER_ROW_CLASS}>
                  <AdminTh>User</AdminTh>
                  <AdminTh align='right'>Orgs</AdminTh>
                  <AdminTh>Status</AdminTh>
                  <AdminTh>Last login</AdminTh>
                  <AdminTh>Joined</AdminTh>
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.map((u) => (
                  <TableRow
                    key={u.id}
                    className={ADMIN_ROW_CLASS}
                    onClick={() => navigate(ROUTES.ADMIN.USER_DETAIL(u.id))}
                  >
                    <TableCell>
                      <div className='flex items-center gap-3'>
                        <div className='flex size-8 items-center justify-center rounded-full bg-muted font-medium text-muted-foreground text-xs uppercase'>
                          {(u.name || u.email).slice(0, 1)}
                        </div>
                        <div className='flex flex-col'>
                          <span className='flex flex-wrap items-center gap-1.5 font-medium'>
                            {u.name || u.email}
                            {/* The three authorities are different in KIND, so they
                                stack rather than collapse into one label. */}
                            {u.is_app_admin && <RoleBadge kind='global_admin' />}
                            {u.top_org_role && <RoleBadge kind={orgRoleKind(u.top_org_role)} />}
                            {/* Delegated cross-org authority via a partner grant —
                                one operator badge per partner they operate. */}
                            {u.partners.map((p) => (
                              <span key={p.id} title={`Partner access at ${p.name}`}>
                                <RoleBadge kind='partner_operator' />
                              </span>
                            ))}
                          </span>
                          <span className='text-muted-foreground text-xs'>{u.email}</span>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className='text-right tabular-nums'>{u.org_count}</TableCell>
                    <TableCell>
                      <AdminStatusPill
                        tone={u.status === "deleted" ? "muted" : "ok"}
                        label={u.status === "deleted" ? "Deactivated" : "Active"}
                      />
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
