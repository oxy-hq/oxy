import { useEffect, useMemo, useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useAllAdminOrgs } from "@/hooks/api/adminTenants/useAdminOrgs";
import { useOxyAccessGrants } from "@/hooks/api/customerApps/useOxyAccessGrants";
import { buildAccessOrgs } from "./accessModel";
import { OrgAccessDetail } from "./OrgAccessDetail";
import {
  AccessStrip,
  EmptyHint,
  GrantsError,
  GrantsLoading,
  ListRow,
  MasterDetail,
  PaneSearch
} from "./shared";

/** Which slice of the org directory the list shows. */
type AccessFilter = "granted" | "none" | "all";

/**
 * Oxy-access cockpit — every org on the deployment, joined with its access
 * grants. Left: the org directory, filterable by Granted / No access / All, so
 * you can see which tenants have opted in AND which haven't. Right: the selected
 * org's granted workspaces (with inline metadata + icon actions), or a "no
 * access" state. A slim stat strip mirrors the Apps tab's FleetStrip.
 *
 * All orgs come from the admin directory (`/admin/orgs-meta`, same owner/
 * app-admin gate as this tab); grants from the access endpoint. Both load up
 * front and join client-side — admin scale is dozens to low hundreds.
 */
export const AccessPane = () => {
  const { data: grants = [], isLoading: grantsLoading, error } = useOxyAccessGrants();
  const {
    data: orgPages,
    isLoading: orgsLoading,
    hasNextPage,
    isFetchingNextPage,
    fetchNextPage
  } = useAllAdminOrgs();
  // Drain every page so the org directory (and the "no access" set + strip
  // counts) is exhaustive, not capped at the first server page.
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);
  const adminOrgs = useMemo(() => orgPages?.pages.flat() ?? [], [orgPages]);
  const accessOrgs = useMemo(() => buildAccessOrgs(adminOrgs, grants), [adminOrgs, grants]);

  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<AccessFilter>("granted");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return accessOrgs.filter((o) => {
      if (filter === "granted" && !o.hasAccess) return false;
      if (filter === "none" && o.hasAccess) return false;
      if (!q) return true;
      if (`${o.orgName} ${o.orgSlug}`.toLowerCase().includes(q)) return true;
      return o.grants.some((g) => g.workspace_name.toLowerCase().includes(q));
    });
  }, [accessOrgs, filter, search]);

  const selected = accessOrgs.find((o) => o.orgId === selectedId) ?? filtered[0] ?? null;
  const withAccess = useMemo(() => accessOrgs.filter((o) => o.hasAccess).length, [accessOrgs]);

  if (error) return <GrantsError error={error} />;
  if (grantsLoading || orgsLoading) return <GrantsLoading />;
  if (accessOrgs.length === 0) {
    return (
      <EmptyHint
        title='No organizations'
        body='No organizations exist on this deployment yet. When one is created it shows up here.'
      />
    );
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <AccessStrip orgs={accessOrgs.length} withAccess={withAccess} workspaces={grants.length} />
      <MasterDetail
        list={
          <div className='flex h-full flex-col'>
            <PaneSearch
              value={search}
              onChange={setSearch}
              placeholder='Search orgs or workspaces…'
            />
            <div className='flex items-center justify-between gap-2 border-border/60 border-b px-2 py-1.5'>
              <Select value={filter} onValueChange={(v) => setFilter(v as AccessFilter)}>
                <SelectTrigger className='h-7 w-auto gap-1 px-2 text-xs' aria-label='Access filter'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent align='start'>
                  <SelectItem value='granted'>Granted</SelectItem>
                  <SelectItem value='none'>No access</SelectItem>
                  <SelectItem value='all'>All orgs</SelectItem>
                </SelectContent>
              </Select>
              <span className='font-mono text-[11px] text-muted-foreground tabular-nums'>
                {filtered.length}
              </span>
            </div>
            <div className='min-h-0 flex-1 overflow-auto'>
              {filtered.length === 0 ? (
                <p className='px-3 py-6 text-center text-muted-foreground text-xs'>
                  No orgs match this filter.
                </p>
              ) : (
                filtered.map((o) => (
                  <ListRow
                    key={o.orgId}
                    active={o.orgId === selected?.orgId}
                    muted={!o.hasAccess}
                    onClick={() => setSelectedId(o.orgId)}
                    title={o.orgName}
                    subtitle={o.orgSlug}
                    trailing={
                      o.hasAccess ? (
                        <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground tabular-nums'>
                          {o.grants.length}
                        </span>
                      ) : (
                        <span className='font-medium text-[10px] text-muted-foreground/50 uppercase tracking-wide'>
                          no access
                        </span>
                      )
                    }
                  />
                ))
              )}
            </div>
          </div>
        }
        detail={
          selected ? (
            <OrgAccessDetail org={selected} highlight={search.trim().toLowerCase()} />
          ) : null
        }
      />
    </div>
  );
};
