import { Building2, RefreshCw, Search } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useAdminOrgsList } from "@/hooks/api/adminTenants/useAdminOrgs";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import { AdminEmptyState } from "../components/AdminEmptyState";
import { AdminStatusPill } from "../components/AdminStatusPill";

/**
 * `/admin/orgs` — operator-grade directory of every organization on this
 * deployment. Compact, scan-first table — name + slug on the left, owner
 * email, member / workspace counts in tabular-nums, created date on the
 * right. Master/detail layout: list with a slide-out sheet on row select.
 *
 * Activity / billing columns will land when the backend exposes
 * `updated_at` and a billing status field on `AdminOrgMeta`.
 */
export default function AdminOrgs() {
  const navigate = useNavigate();
  const [search, setSearch] = useState("");
  const [searchInput, setSearchInput] = useState("");
  const { data: orgs = [], isLoading, isFetching, refetch } = useAdminOrgsList({ search });

  return (
    <div className='mx-auto max-w-7xl space-y-6 p-6 lg:px-10 lg:py-10'>
      <header className='space-y-2'>
        <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          Admin · Tenants ·{" "}
          <Link to={ROUTES.ADMIN.TENANTS} className='hover:text-foreground'>
            Overview
          </Link>{" "}
          / Organizations
        </p>
        <h1 className='font-semibold text-3xl tracking-tight'>Organizations</h1>
        <p className='max-w-2xl text-muted-foreground text-sm'>
          Every organization on this deployment. Select a row to inspect, rename, or transfer
          ownership. Use the overview hub for fleet-wide health.
        </p>
      </header>

      <div className='overflow-hidden rounded-lg border border-border/60 bg-card'>
        {/* Filter strip */}
        <div className='flex items-center justify-between gap-3 border-border/60 border-b px-4 py-3'>
          <form
            className='relative w-full max-w-sm'
            onSubmit={(e) => {
              e.preventDefault();
              setSearch(searchInput.trim());
            }}
          >
            <Search className='absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground' />
            <Input
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              placeholder='Search by name or slug'
              className='h-8 pl-7 text-sm'
            />
          </form>
          <div className='flex items-center gap-3'>
            <span className='font-medium text-[10px] text-muted-foreground uppercase tabular-nums tracking-[0.14em]'>
              {isLoading
                ? "…"
                : `${orgs.length.toLocaleString()} org${orgs.length === 1 ? "" : "s"}`}
            </span>
            <Button
              variant='ghost'
              size='sm'
              onClick={() => refetch()}
              disabled={isFetching}
              className='h-8'
            >
              <RefreshCw className={cn("size-3.5", isFetching && "animate-spin")} />
              Refresh
            </Button>
          </div>
        </div>

        {/* Body */}
        {isLoading ? (
          <div className='flex items-center justify-center gap-2 py-20 text-muted-foreground text-sm'>
            <Spinner /> Loading organizations…
          </div>
        ) : orgs.length === 0 ? (
          <div className='p-6'>
            <AdminEmptyState
              icon={Building2}
              title={search ? `No organizations match "${search}".` : "No organizations yet"}
              description={
                search
                  ? "Try a different search term, or clear the filter."
                  : "Organizations are created via the signup flow or by an admin from /admin/orgs."
              }
            />
          </div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className='border-border/60'>
                <TableHead className='font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
                  Organization
                </TableHead>
                <TableHead className='font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
                  Owner
                </TableHead>
                <TableHead className='font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
                  Status
                </TableHead>
                <TableHead className='text-right font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
                  Members
                </TableHead>
                <TableHead className='text-right font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
                  Workspaces
                </TableHead>
                <TableHead className='font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
                  Created
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orgs.map((org) => (
                <TableRow
                  key={org.id}
                  className='cursor-pointer border-border/60 transition-colors hover:bg-muted/40'
                  onClick={() => navigate(ROUTES.ADMIN.ORG_DETAIL(org.id))}
                >
                  <TableCell>
                    <div className='flex items-center gap-3'>
                      <div className='flex size-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-muted/40 text-muted-foreground'>
                        <Building2 className='size-3.5' />
                      </div>
                      <div className='flex min-w-0 flex-col'>
                        <span className='truncate font-medium'>{org.name}</span>
                        <span className='truncate font-mono text-[10px] text-muted-foreground'>
                          /{org.slug}
                        </span>
                      </div>
                    </div>
                  </TableCell>
                  <TableCell className='font-mono text-[11px] text-muted-foreground'>
                    {org.owner_email ?? "—"}
                  </TableCell>
                  <TableCell>
                    <AdminStatusPill
                      tone={org.member_count > 0 ? "ok" : "muted"}
                      label={org.member_count > 0 ? "Active" : "Empty"}
                    />
                  </TableCell>
                  <TableCell className='text-right font-medium text-sm tabular-nums'>
                    {org.member_count.toLocaleString()}
                  </TableCell>
                  <TableCell className='text-right font-medium text-sm tabular-nums'>
                    {org.workspace_count.toLocaleString()}
                  </TableCell>
                  <TableCell className='text-muted-foreground text-xs tabular-nums'>
                    {new Date(org.created_at).toLocaleDateString(undefined, {
                      year: "numeric",
                      month: "short",
                      day: "numeric"
                    })}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>
    </div>
  );
}
