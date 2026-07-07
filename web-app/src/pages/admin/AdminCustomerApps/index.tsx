import { isAxiosError } from "axios";
import { useEffect, useMemo, useState } from "react";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useAdminApps } from "@/hooks/api/customerApps/useCustomerApps";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { AppDetailPage } from "./components/AppDetailPage";
import { AppsTable } from "./components/AppsTable";
import { CreateCustomerAppDialog } from "./components/CreateCustomerAppDialog";
import { AccessPane } from "./components/OxyAccessPanes/AccessPane";

/**
 * Admin console for customer apps + the Oxy-access browser.
 *
 * Two tabs share the page shell, selected via `?view=`:
 *   apps   (default) → customer-app registry as a full-width management table;
 *                      `/admin/apps/:org/:app` opens the full-page detail.
 *   access           → orgs that granted Oxy access, with their workspaces +
 *                      grant metadata inline.
 *
 * A selected app (the `/admin/apps/:orgSlug/:appSlug` route) always implies
 * the Apps tab, so deep links keep working regardless of `?view`. Legacy
 * `?view=orgs` / `?view=projects` links fold into the unified Access tab.
 */
type View = "apps" | "access";

// The Access label intentionally stays distinct from the cross-cutting tenant
// directory under /admin/orgs and /admin/workspaces — this surface only lists
// orgs/workspaces that have granted Oxy access for customer-app management.
const TABS: { view: View; label: string; to: string }[] = [
  { view: "apps", label: "Apps", to: "/admin/apps" },
  { view: "access", label: "Access", to: "/admin/apps?view=access" }
];

export default function AdminCustomerApps() {
  const [searchParams] = useSearchParams();
  const params = useParams<{ orgSlug?: string; appSlug?: string }>();
  const view: View = params.appSlug ? "apps" : normalizeView(searchParams.get("view"));

  return (
    <div className='flex h-[calc(100vh-3.5rem)] flex-col'>
      <AdminTabs active={view} />
      {view === "apps" ? <AppsPane /> : <AccessPane />}
    </div>
  );
}

// `orgs` / `projects` are accepted for backward compatibility with bookmarked
// links from the previous three-tab layout; both resolve to the merged Access
// view.
const normalizeView = (v: string | null): View =>
  v === "access" || v === "orgs" || v === "projects" ? "access" : "apps";

const AdminTabs = ({ active }: { active: View }) => (
  <div className='flex items-center gap-1 border-border border-b px-2'>
    {TABS.map((t) => (
      <Link
        key={t.view}
        to={t.to}
        className={cn(
          "-mb-px border-b-2 px-2.5 py-1.5 text-xs transition-colors",
          active === t.view
            ? "border-primary font-medium text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        )}
      >
        {t.label}
      </Link>
    ))}
  </div>
);

/**
 * Customer-app registry. A full-width table is the management surface; the
 * rich per-app dossier opens as a full page (`AppDetailPage`) keyed by the
 * selected-app URL segments (so deep links and the back button still work).
 *
 * All pages are loaded up front so filter / sort / group operate over the
 * whole registry rather than just the first page — admin scale is dozens to
 * low hundreds, so a handful of background fetches is cheap. Revisit with
 * server-side querying only if the registry ever grows into the thousands.
 */
const AppsPane = () => {
  const { data, isLoading, error, hasNextPage, isFetchingNextPage, fetchNextPage } =
    useAdminApps(100);
  const apps = useMemo(() => data?.pages.flatMap((p) => p.items) ?? [], [data]);
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams<{ orgSlug?: string; appSlug?: string }>();
  const [createOpen, setCreateOpen] = useState(false);

  // Walk the remaining pages automatically so the table sees every app.
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const selectedKey = useMemo(() => {
    if (!params.orgSlug || !params.appSlug) return null;
    return `${params.orgSlug}/${params.appSlug}`;
  }, [params.orgSlug, params.appSlug]);

  const selected = useMemo(
    () => apps.find((a) => `${a.org_slug}/${a.slug}` === selectedKey) ?? null,
    [apps, selectedKey]
  );

  // If the URL referenced an app that no longer exists, drop the phantom
  // selection once loading settles. Preserve the table's query state.
  useEffect(() => {
    if (selectedKey && !isLoading && apps.length > 0 && !selected) {
      navigate({ pathname: "/admin/apps", search: location.search }, { replace: true });
    }
  }, [selectedKey, selected, isLoading, apps.length, navigate, location.search]);

  const openDetail = (app: CustomerApp) =>
    navigate({
      pathname: `/admin/apps/${app.org_slug}/${app.slug}`,
      search: location.search
    });

  const closeDetail = () => navigate({ pathname: "/admin/apps", search: location.search });

  if (error && !isLoading) return <ErrorState error={error} />;

  // A selected app takes over the whole pane as its own page (Vercel-style),
  // rather than sliding in over the list. Deep links resolve once loading
  // settles; until then the list (with its own loading state) shows.
  if (selected) return <AppDetailPage app={selected} onBack={closeDetail} />;

  return (
    <>
      <AppsTable
        apps={apps}
        isLoading={isLoading}
        isLoadingMore={isFetchingNextPage}
        onSelect={openDetail}
        onCreate={() => setCreateOpen(true)}
      />
      <CreateCustomerAppDialog open={createOpen} onOpenChange={setCreateOpen} />
    </>
  );
};

const ErrorState = ({ error }: { error: unknown }) => (
  <div className='mx-auto max-w-2xl p-6'>
    <div className='rounded-lg border border-destructive/30 bg-destructive/5 p-6 text-center'>
      {isAxiosError(error) && error.response?.status === 403 ? (
        <>
          <p className='font-medium text-destructive text-sm'>
            Your account isn't on the customer-apps allow list.
          </p>
          <p className='mt-2 text-muted-foreground text-xs'>
            Add your email to the oxy backend's{" "}
            <code className='rounded bg-muted px-1 py-0.5 font-mono'>OXY_GLOBAL_ADMINS</code> env
            var (comma-separated) and restart the server, then refresh.
          </p>
        </>
      ) : (
        <p className='text-destructive text-sm'>Failed to load apps.</p>
      )}
    </div>
  </div>
);
