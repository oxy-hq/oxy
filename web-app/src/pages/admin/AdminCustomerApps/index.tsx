import { isAxiosError } from "axios";
import { AppWindow } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAdminApps } from "@/hooks/api/customerApps/useCustomerApps";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { AppDetail } from "./components/AppDetail";
import { AppList } from "./components/AppList";
import { CreateCustomerAppDialog } from "./components/CreateCustomerAppDialog";
import { OrgsPane } from "./components/OxyAccessPanes/OrgsPane";
import { ProjectsPane } from "./components/OxyAccessPanes/ProjectsPane";

/**
 * Admin console for customer apps + the Oxy-access org/project browser.
 *
 * Three tabs share the master-detail shell, selected via `?view=`:
 *   apps     (default) → customer-app registry; `/admin/apps/:org/:app` for detail
 *   orgs              → orgs that granted Oxy access → their projects
 *   projects          → flat list of granted workspaces
 *
 * A selected app (the `/admin/apps/:orgSlug/:appSlug` route) always implies
 * the Apps tab, so deep links keep working regardless of `?view`.
 */
type View = "apps" | "orgs" | "projects";

// Labels on the Orgs / Projects tabs intentionally call out *access* so
// they don't get confused with the cross-cutting tenant directory under
// /admin/orgs and /admin/workspaces. These panes only list orgs and
// projects that have granted Oxy access for customer-app management.
const TABS: { view: View; label: string; to: string }[] = [
  { view: "apps", label: "Apps", to: "/admin/apps" },
  { view: "orgs", label: "Orgs with access", to: "/admin/apps?view=orgs" },
  { view: "projects", label: "Projects with access", to: "/admin/apps?view=projects" }
];

export default function AdminCustomerApps() {
  const [searchParams] = useSearchParams();
  const params = useParams<{ orgSlug?: string; appSlug?: string }>();
  const view: View = params.appSlug ? "apps" : normalizeView(searchParams.get("view"));

  return (
    <div className='flex h-[calc(100vh-3.5rem)] flex-col'>
      <AdminTabs active={view} />
      {view === "apps" ? <AppsPane /> : view === "orgs" ? <OrgsPane /> : <ProjectsPane />}
    </div>
  );
}

const normalizeView = (v: string | null): View => (v === "orgs" || v === "projects" ? v : "apps");

const AdminTabs = ({ active }: { active: View }) => (
  <div className='flex items-center gap-1 border-border border-b px-2'>
    {TABS.map((t) => (
      <Link
        key={t.view}
        to={t.to}
        className={cn(
          "border-b-2 px-3 py-2 text-sm transition-colors",
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
 * Customer-app registry master-detail. Resizable (admin can grow the
 * preview iframe) defaulting to 28/72 so the list pane fits org/slug pairs.
 */
const AppsPane = () => {
  const { data, isLoading, error, hasNextPage, isFetchingNextPage, fetchNextPage } = useAdminApps();
  // Pages come newest-first and we walk back in time, so concat order is
  // display order.
  const apps = useMemo(() => data?.pages.flatMap((p) => p.items) ?? [], [data]);
  const navigate = useNavigate();
  const params = useParams<{ orgSlug?: string; appSlug?: string }>();
  const [createOpen, setCreateOpen] = useState(false);

  const selectedKey = useMemo(() => {
    if (!params.orgSlug || !params.appSlug) return null;
    return `${params.orgSlug}/${params.appSlug}`;
  }, [params.orgSlug, params.appSlug]);

  const selected = useMemo(
    () => apps.find((a) => `${a.org_slug}/${a.slug}` === selectedKey) ?? null,
    [apps, selectedKey]
  );

  // If the URL referenced an app that no longer exists, drop back to the
  // empty state rather than leaving a phantom selection.
  useEffect(() => {
    if (selectedKey && !isLoading && apps.length > 0 && !selected) {
      navigate("/admin/apps", { replace: true });
    }
  }, [selectedKey, selected, isLoading, apps.length, navigate]);

  const handleSelect = (app: CustomerApp) => {
    navigate(`/admin/apps/${app.org_slug}/${app.slug}?tab=preview`);
  };

  if (error && !isLoading) {
    return <ErrorState error={error} />;
  }

  return (
    <>
      <ResizablePanelGroup direction='horizontal' className='min-h-0 flex-1'>
        <ResizablePanel defaultSize={28} minSize={18} maxSize={45}>
          <AppList
            apps={apps}
            isLoading={isLoading}
            selectedKey={selectedKey}
            onSelect={handleSelect}
            onCreate={() => setCreateOpen(true)}
            hasMore={hasNextPage ?? false}
            isLoadingMore={isFetchingNextPage}
            onLoadMore={() => fetchNextPage()}
          />
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={72} minSize={40}>
          {selected ? <AppDetail app={selected} /> : <EmptyDetail isLoading={isLoading} />}
        </ResizablePanel>
      </ResizablePanelGroup>

      <CreateCustomerAppDialog open={createOpen} onOpenChange={setCreateOpen} />
    </>
  );
};

const EmptyDetail = ({ isLoading }: { isLoading: boolean }) => (
  <div className='flex h-full flex-col items-center justify-center gap-3 bg-muted/20 px-6 text-center'>
    {isLoading ? (
      <Spinner className='size-5' />
    ) : (
      <>
        <div className='flex size-12 items-center justify-center rounded-full border bg-background shadow-sm'>
          <AppWindow className='size-5 text-muted-foreground' />
        </div>
        <div>
          <p className='font-medium text-foreground text-sm'>Pick an app to inspect</p>
          <p className='mt-1 max-w-sm text-muted-foreground text-sm'>
            Live preview, manifest snapshot, sync + delete — all here. Or hit{" "}
            <kbd className='rounded border bg-muted px-1 py-0.5 font-mono text-xs'>+</kbd> to
            bootstrap a new one.
          </p>
        </div>
      </>
    )}
  </div>
);

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
