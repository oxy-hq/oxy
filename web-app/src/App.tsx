import "@/styles/shadcn/index.css";
import {
  createBrowserRouter,
  createRoutesFromElements,
  Navigate,
  Outlet,
  Route,
  RouterProvider,
  Routes,
  useNavigate,
  useParams
} from "react-router-dom";
import { AppSidebar } from "@/components/AppSidebar";
import { SidebarProvider } from "@/components/ui/shadcn/sidebar";
import { Toaster as ShadcnToaster } from "@/components/ui/shadcn/sonner";
import AirwayPage from "@/pages/airway";
import Home from "@/pages/home";
import ThreadPage from "@/pages/thread";
import Threads from "@/pages/threads";
import WorkflowPage from "@/pages/workflow";
import WorkflowsListPage from "@/pages/workflow/WorkflowsListPage";
import "@xyflow/react/dist/style.css";
import React, { Suspense, useEffect, useRef } from "react";
import { HotkeysProvider, useHotkeys } from "react-hotkeys-hook";
import { toast } from "sonner";
import { Spinner } from "@/components/ui/shadcn/spinner";
import ROUTES from "@/libs/utils/routes";
import ContextGraphPage from "@/pages/context-graph";
import { ErrorBoundary } from "@/sentry";
import { BuilderDialog } from "./components/BuilderDialog";
import { FileQuickOpen } from "./components/FileQuickOpen";
import OrgGuard from "./components/OrgGuard";
import OwnerRedirect from "./components/OwnerRedirect";
import ProtectedRoute from "./components/ProtectedRoute";
import SettingsDialog from "./components/settings/SettingsDialog";
import WorkspaceStatus from "./components/WorkspaceStatus";
import AgenticSetupPage from "./components/workspaces/components/CreateWorkspaceDialog/components/AgenticSetup";
import { LocalWorkspaceSetupDialog } from "./components/workspaces/components/LocalWorkspaceSetupDialog";
import { ManageWorkspacesDialog } from "./components/workspaces/components/ManageWorkspacesDialog";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { useWorkspace } from "./hooks/api/workspaces/useWorkspaces";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import { LOCAL_WORKSPACE_ID } from "./libs/utils/constants";
import { setLastWorkspaceId } from "./libs/utils/lastWorkspace";
import AppPage from "./pages/app";
import GoogleCallback from "./pages/auth/GoogleCallback";
import MagicLinkCallback from "./pages/auth/MagicLinkCallback";
import OktaCallback from "./pages/auth/OktaCallback";
import GitHubCallback from "./pages/github/callback";
import InvitePage from "./pages/Invite";
import LoginPage from "./pages/login";
import OrgDispatcher from "./pages/OrgDispatcher";
import OnboardingPage from "./pages/onboarding";
import OrgOnboardingPage from "./pages/onboarding/OrgOnboardingPage";
import PostLoginDispatcher from "./pages/PostLoginDispatcher";
import useBuilderDialog from "./stores/useBuilderDialog";
import useCurrentOrg from "./stores/useCurrentOrg";
import useCurrentWorkspace from "./stores/useCurrentWorkspace";
import useFileQuickOpen from "./stores/useFileQuickOpen";
import type { AuthConfigResponse } from "./types/auth";

// Lazy-load the entire IDE subtree. The IDE pulls in Monaco, monaco-yaml,
// and the editors vendor chunk (~300-400KB gzipped); users who never visit
// /ide should not pay for it on initial load.
const IdePage = React.lazy(() => import("./pages/ide"));
const FilesLayout = React.lazy(() => import("./pages/ide/Files"));
const EditorPage = React.lazy(() => import("./pages/ide/Files/Editor"));
const LookerExplorerPage = React.lazy(() => import("./pages/ide/Files/Editor/LookerExplore"));
const DatabaseLayout = React.lazy(() => import("./pages/ide/Database"));
const QueryWorkspacePage = React.lazy(() => import("./pages/ide/Database/QueryWorkspace"));
const ModelingPage = React.lazy(() => import("./pages/ide/modeling"));
const PipelinesPage = React.lazy(() => import("./pages/ide/pipelines"));
const TestsLayout = React.lazy(() => import("./pages/ide/tests"));
const TestsDashboardPage = React.lazy(() => import("./pages/ide/tests/TestsDashboardPage"));
const TestsRunsPage = React.lazy(() => import("./pages/ide/tests/TestsRunsPage"));
const TestFileDetailPage = React.lazy(() => import("./pages/ide/tests/TestFileDetailPage"));
const CoordinatorLayout = React.lazy(() => import("./pages/ide/coordinator"));
const ActiveRunsPage = React.lazy(() => import("./pages/ide/coordinator/ActiveRuns"));
const RunHistoryPage = React.lazy(() => import("./pages/ide/coordinator/RunHistory"));
const RecoveryPage = React.lazy(() => import("./pages/ide/coordinator/Recovery"));
const QueueHealthPage = React.lazy(() => import("./pages/ide/coordinator/QueueHealth"));
const RunTreePage = React.lazy(() => import("./pages/ide/coordinator/RunTree"));
const SchedulesPage = React.lazy(() => import("./pages/ide/coordinator/Schedules"));
const ObservabilityLayout = React.lazy(() => import("./pages/ide/observability"));
const ExecutionAnalytics = React.lazy(
  () => import("./pages/ide/observability/execution-analytics")
);
const ClusterMapPage = React.lazy(() => import("./pages/ide/observability/clusters"));
const MetricDetailPage = React.lazy(
  () => import("./pages/ide/observability/metrics/MetricsDetailPage")
);
const MetricsPage = React.lazy(() => import("./pages/ide/observability/metrics/MetricsListPage"));
const TraceDetailPage = React.lazy(() => import("./pages/ide/observability/trace"));
const TracesPage = React.lazy(() => import("./pages/ide/observability/traces"));

// Admin and Stripe return URLs are visited rarely; defer their bundles too.
const AdminLayout = React.lazy(() => import("./pages/admin/AdminLayout"));
const AdminBillingQueue = React.lazy(() => import("./pages/admin/AdminBillingQueue"));
const AdminFeatureFlags = React.lazy(() => import("./pages/admin/AdminFeatureFlags"));
const CheckoutSuccessPage = React.lazy(() => import("./pages/billing/CheckoutSuccess"));
const CheckoutCancelledPage = React.lazy(() => import("./pages/billing/CheckoutCancelled"));

const RouteFallback = () => (
  <div className='flex h-full w-full items-center justify-center'>
    <Spinner className='size-6' />
  </div>
);

const MainPageWrapper = ({ children }: { children: React.ReactNode }) => {
  return (
    <main className='flex h-full w-full min-w-0 flex-1 flex-col bg-background'>
      <WorkspaceStatus />
      <div className='w-full min-w-0 flex-1 overflow-hidden'>{children}</div>
    </main>
  );
};

const WorkspaceLayout = React.memo(function WorkspaceLayout() {
  const { authConfig, isLocalMode } = useAuth();
  const { wsId: wsIdParam } = useParams<{ wsId: string }>();
  const orgSlug = useCurrentOrg((s) => s.org?.slug);
  const orgId = useCurrentOrg((s) => s.org?.id);
  const navigate = useNavigate();

  // In local mode the router doesn't carry a :wsId segment — the single
  // implicit workspace is addressed by the nil UUID.
  const wsId = isLocalMode ? LOCAL_WORKSPACE_ID : wsIdParam;
  // biome-ignore lint/style/noNonNullAssertion: local gets the constant, cloud gets the :wsId param
  const { isPending, isError, error, data } = useWorkspace(wsId!);
  const { setWorkspace, workspace } = useCurrentWorkspace();

  const { setIsOpen: setBuilderDialogOpen } = useBuilderDialog();
  const { setIsOpen: setFileQuickOpenOpen } = useFileQuickOpen();
  useHotkeys("meta+i", () => setBuilderDialogOpen(!useBuilderDialog.getState().isOpen), {
    preventDefault: true,
    useKey: true
  });
  useHotkeys("meta+p", () => setFileQuickOpenOpen(true), { preventDefault: true, useKey: true });

  React.useEffect(() => {
    if (!isPending && !isError && data) {
      setWorkspace(data);
    }
  }, [isPending, isError, setWorkspace, data]);

  // Remember the last-opened workspace per-org so the post-login dispatcher
  // can skip the picker next time. Skipped in local mode (no real orgs).
  React.useEffect(() => {
    if (isLocalMode) return;
    if (!isPending && !isError && data && orgId && wsId) {
      setLastWorkspaceId(orgId, wsId);
    }
  }, [isPending, isError, data, orgId, wsId, isLocalMode]);

  // In local mode there's nowhere to redirect to — surface the error via toast
  // and let the caller see the empty layout. The cloud fallbacks below don't apply.
  React.useEffect(() => {
    if (!isPending && data?.workspace_error) {
      toast.error(data.workspace_error);
      if (isLocalMode) return;
      if (orgSlug) {
        navigate(ROUTES.ORG(orgSlug).ROOT, { replace: true });
      } else {
        navigate(ROUTES.ROOT, { replace: true });
      }
    }
  }, [isPending, data?.workspace_error, orgSlug, navigate, isLocalMode]);

  useEffect(() => {
    if (isError) {
      const msg =
        (error as { response?: { data?: { error?: string } } })?.response?.data?.error ??
        "Failed to load workspace.";
      toast.error(msg);
      if (isLocalMode) return;
      if (orgSlug) {
        navigate(ROUTES.ORG(orgSlug).ROOT, { replace: true });
      } else {
        navigate(ROUTES.ROOT, { replace: true });
      }
    }
  }, [isError, error, navigate, orgSlug, isLocalMode]);

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner />
      </div>
    );
  }

  // When a local-mode server has no config.yml, render a blocking setup
  // dialog instead of the main shell. Short-circuits before the sidebar /
  // IDE / routes mount, so WorkspaceManager-dependent endpoints are never
  // called (they would 503). `WorkspaceStatus` is not mounted in this path
  // either — it would surface config errors as a banner, which is the
  // wrong UX for the first-run case.
  if (isLocalMode && data?.requires_local_setup) {
    return <LocalWorkspaceSetupDialog />;
  }

  if (isError || !workspace) {
    return null;
  }

  return (
    <HotkeysProvider>
      <BuilderDialog />
      <FileQuickOpen />
      <SettingsDialog />
      <ManageWorkspacesDialog />
      <AppSidebar />

      <Routes>
        <Route
          index
          element={
            <MainPageWrapper>
              <Home />
            </MainPageWrapper>
          }
        />

        <Route
          path='home'
          element={
            <MainPageWrapper>
              <Home />
            </MainPageWrapper>
          }
        />
        <Route
          path='threads'
          element={
            <MainPageWrapper>
              <Threads />
            </MainPageWrapper>
          }
        />
        <Route
          path='threads/:threadId'
          element={
            <MainPageWrapper>
              <ThreadPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='workflows'
          element={
            <MainPageWrapper>
              <WorkflowsListPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='workflows/:pathb64'
          element={
            <MainPageWrapper>
              <WorkflowPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='pipelines/:pathb64'
          element={
            <MainPageWrapper>
              <AirwayPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='pipelines/:pathb64/runs/:runId'
          element={
            <MainPageWrapper>
              <AirwayPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='apps/:pathb64'
          element={
            <MainPageWrapper>
              <AppPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='ide'
          element={
            <Suspense fallback={<RouteFallback />}>
              <IdePage />
            </Suspense>
          }
        >
          {/* Files routes */}
          <Route path='files' element={<FilesLayout />}>
            <Route path=':pathb64' element={<EditorPage />} />
            <Route
              path='looker/:integrationName/:model/:exploreName'
              element={<LookerExplorerPage />}
            />
          </Route>

          {/* Database routes */}
          <Route path='database' element={<DatabaseLayout />}>
            <Route index element={<QueryWorkspacePage />} />
          </Route>

          {/* Data Modeling routes */}
          <Route path='modeling' element={<ModelingPage />} />

          {/* Airway pipeline routes */}
          <Route path='pipelines' element={<PipelinesPage />} />

          {/* Tests routes */}
          <Route path='tests' element={<TestsLayout />}>
            <Route index element={<TestsDashboardPage />} />
            <Route path='runs' element={<TestsRunsPage />} />
            <Route path=':pathb64' element={<TestFileDetailPage />} />
          </Route>

          {/* Coordinator routes */}
          <Route path='coordinator' element={<CoordinatorLayout />}>
            <Route path='active-runs' element={<ActiveRunsPage />} />
            <Route path='run-history' element={<RunHistoryPage />} />
            <Route path='schedules' element={<SchedulesPage />} />
            <Route path='recovery' element={<RecoveryPage />} />
            <Route path='queue' element={<QueueHealthPage />} />
            <Route path='runs/:runId/tree' element={<RunTreePage />} />
            <Route index element={<Navigate to='active-runs' replace />} />
          </Route>

          {/* Observability routes (enterprise only) */}
          {authConfig.enterprise && (
            <Route path='observability' element={<ObservabilityLayout />}>
              <Route path='traces' element={<TracesPage />} />
              <Route path='traces/:traceId' element={<TraceDetailPage />} />
              <Route path='clusters' element={<ClusterMapPage />} />
              <Route path='metrics' element={<MetricsPage />} />
              <Route path='metrics/:metricName' element={<MetricDetailPage />} />
              <Route path='execution-analytics' element={<ExecutionAnalytics />} />
            </Route>
          )}

          {/* Default redirect to files */}
          <Route index element={<Navigate to='files' replace />} />
        </Route>
        <Route
          path='onboarding'
          element={
            <MainPageWrapper>
              <AgenticSetupPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='context-graph'
          element={
            <MainPageWrapper>
              <ContextGraphPage />
            </MainPageWrapper>
          }
        />

        <Route path='*' element={<Navigate to='.' />} />
      </Routes>
    </HotkeysProvider>
  );
});

/** Local-mode router: a flat shape with the implicit workspace mounted at `/`.
 *  Mirrors the backend's local-mode route set (no org, no login, no workspace
 *  management). Any path the user visits that isn't a workspace sub-route
 *  falls through to the `*` handler inside `WorkspaceLayout` and lands on `/`. */
const getLocalRouter = () =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route
        path='/*'
        element={
          <ProtectedRoute>
            <SidebarProvider>
              <WorkspaceLayout />
            </SidebarProvider>
          </ProtectedRoute>
        }
      />
    )
  );

const getCloudRouter = (authConfig: AuthConfigResponse) =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route>
        {/* Auth routes when auth is enabled */}
        {authConfig.auth_enabled && (
          <>
            <Route path={ROUTES.AUTH.LOGIN} element={<LoginPage />} />
            <Route path={ROUTES.AUTH.GOOGLE_CALLBACK} element={<GoogleCallback />} />
            <Route path={ROUTES.AUTH.OKTA_CALLBACK} element={<OktaCallback />} />
            <Route path={ROUTES.AUTH.MAGIC_LINK_CALLBACK} element={<MagicLinkCallback />} />
          </>
        )}

        {/* GitHub callback must always be accessible (used during the workspace import popup flow) */}
        <Route path='/github/callback' element={<GitHubCallback />} />

        {/* Invitation accept — public; the page itself redirects to /login if needed */}
        <Route path='/invite/:token' element={<InvitePage />} />

        {/* Auth-gated routes */}
        <Route
          path='/*'
          element={
            <ProtectedRoute>
              <Outlet />
            </ProtectedRoute>
          }
        >
          {/* Admin queue (OXY_OWNER-gated server-side) — sits outside
              `OwnerRedirect` so owners can actually reach it. */}
          <Route
            element={
              <Suspense fallback={<RouteFallback />}>
                <AdminLayout />
              </Suspense>
            }
          >
            <Route path='admin/billing/queue' element={<AdminBillingQueue />} />
            <Route path='admin/feature-flags' element={<AdminFeatureFlags />} />
          </Route>

          {/* User-facing routes — owners get bounced to the admin queue. */}
          <Route element={<OwnerRedirect />}>
            {/* Top-level: smart dispatcher picks onboarding / last workspace / first workspace */}
            <Route index element={<PostLoginDispatcher />} />
            <Route path='onboarding' element={<OnboardingPage />} />

            {/* Org-scoped routes */}
            <Route path=':orgSlug' element={<OrgGuard />}>
              {/* Org onboarding (first workspace + optional invites) — no sidebar */}
              <Route path='onboarding' element={<OrgOnboardingPage />} />

              {/* Stripe Checkout return URLs. The path includes `/billing/`,
                  which is the `OrgGuard` paywall bypass — these pages
                  render even while billing.status is `incomplete`. */}
              <Route
                path='billing/checkout-success'
                element={
                  <Suspense fallback={<RouteFallback />}>
                    <CheckoutSuccessPage />
                  </Suspense>
                }
              />
              <Route
                path='billing/checkout-cancelled'
                element={
                  <Suspense fallback={<RouteFallback />}>
                    <CheckoutCancelledPage />
                  </Suspense>
                }
              />

              {/* Org root picks a workspace and redirects into it */}
              <Route index element={<OrgDispatcher />} />

              {/* Workspace-scoped routes */}
              <Route
                path='workspaces/:wsId/*'
                element={
                  <SidebarProvider>
                    <WorkspaceLayout />
                  </SidebarProvider>
                }
              />
            </Route>
          </Route>
        </Route>
      </Route>
    )
  );

const getRouter = (authConfig: AuthConfigResponse) =>
  authConfig.mode === "local" ? getLocalRouter() : getCloudRouter(authConfig);

function App() {
  const { data: authConfig, isPending } = useAuthConfig();

  // Only recreate the router when routing-relevant fields change — prevents the
  // router from being torn down on every authConfig refetch (e.g. when a GitHub
  // popup closes and the window regains focus), which would reset page state.
  const routerRef = useRef<ReturnType<typeof getRouter> | null>(null);
  const prevRouterKey = useRef<string | null>(null);
  const routerKey = authConfig ? `${authConfig.auth_enabled}:${authConfig.mode}` : null;
  if (authConfig && routerKey !== prevRouterKey.current) {
    routerRef.current = getRouter(authConfig);
    prevRouterKey.current = routerKey;
  }
  const router = routerRef.current;

  if (isPending || !authConfig || !router) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  return (
    <ErrorBoundary fallback={<div>Something went wrong. Please refresh.</div>} showDialog>
      <AuthProvider authConfig={authConfig}>
        <RouterProvider router={router} />
        <ShadcnToaster />
      </AuthProvider>
    </ErrorBoundary>
  );
}

export default App;
