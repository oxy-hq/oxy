import { FolderOpen, RefreshCw, Search } from "lucide-react";
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
import { useAdminWorkspacesList } from "@/hooks/api/adminTenants/useAdminWorkspaces";
import ROUTES from "@/libs/utils/routes";
import type { WorkspaceStatusId } from "@/services/api/adminTenants";

type StatusFilter = "all" | WorkspaceStatusId;

const STATUS_VARIANT: Record<
  WorkspaceStatusId,
  "default" | "secondary" | "destructive" | "outline"
> = {
  ready: "default",
  cloning: "secondary",
  failed: "destructive",
  not_oxy_project: "outline"
};

/**
 * `/admin/workspaces` — OXY_OWNER-only directory of every workspace
 * across every organization.
 */
export default function AdminWorkspaces() {
  const navigate = useNavigate();
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<StatusFilter>("all");

  const {
    data: workspaces = [],
    isLoading,
    isFetching,
    refetch
  } = useAdminWorkspacesList({
    search,
    status: status === "all" ? undefined : status
  });

  return (
    <div className='mx-auto max-w-6xl p-6'>
      <div className='mb-6'>
        <h1 className='font-semibold text-2xl tracking-tight'>Workspaces</h1>
        <p className='mt-1 text-muted-foreground text-sm'>
          Every workspace across every organization. Search, filter by status, and operate on the
          membership.
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
                placeholder='Search by workspace name'
                className='pl-8'
              />
            </form>

            <Select value={status} onValueChange={(v) => setStatus(v as StatusFilter)}>
              <SelectTrigger className='w-36'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='all'>All statuses</SelectItem>
                <SelectItem value='ready'>Ready</SelectItem>
                <SelectItem value='cloning'>Cloning</SelectItem>
                <SelectItem value='failed'>Failed</SelectItem>
                <SelectItem value='not_oxy_project'>Not Oxy project</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className='flex items-center gap-3'>
            {!isLoading ? (
              <span className='text-muted-foreground text-sm'>
                {workspaces.length} {workspaces.length === 1 ? "workspace" : "workspaces"}
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
          ) : workspaces.length === 0 ? (
            <EmptyState search={search} />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Workspace</TableHead>
                  <TableHead>Organization</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className='text-right'>Members</TableHead>
                  <TableHead>Last opened</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {workspaces.map((w) => (
                  <TableRow
                    key={w.id}
                    className='cursor-pointer hover:bg-muted/40'
                    onClick={() => navigate(ROUTES.ADMIN.WORKSPACE_DETAIL(w.id))}
                  >
                    <TableCell>
                      <div className='flex items-center gap-3'>
                        <div className='flex size-8 items-center justify-center rounded-md bg-muted text-muted-foreground'>
                          <FolderOpen className='size-4' />
                        </div>
                        <div className='flex flex-col'>
                          <span className='font-medium'>{w.name}</span>
                          <span className='text-muted-foreground text-xs'>
                            Created {new Date(w.created_at).toLocaleDateString()}
                          </span>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm'>
                      {w.org_slug ? `/${w.org_slug}` : <span className='italic'>orphaned</span>}
                    </TableCell>
                    <TableCell>
                      <Badge variant={STATUS_VARIANT[w.status]} className='gap-1.5'>
                        <span className='size-1.5 rounded-full bg-current opacity-70' />
                        {w.status === "not_oxy_project" ? "not oxy" : w.status}
                      </Badge>
                    </TableCell>
                    <TableCell className='text-right tabular-nums'>{w.member_count}</TableCell>
                    <TableCell className='text-muted-foreground text-sm tabular-nums'>
                      {w.last_opened_at ? new Date(w.last_opened_at).toLocaleDateString() : "—"}
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
      <FolderOpen className='size-8' />
      <p className='text-sm'>
        {search ? `No workspaces match "${search}".` : "No workspaces yet."}
      </p>
      <p className='text-xs'>
        {search
          ? "Try a different search term, or clear the filter."
          : "Workspaces appear here as organizations import or create projects."}
      </p>
    </div>
  );
}
