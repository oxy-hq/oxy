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
import Home from "@/pages/home";
import ClusterMapPage from "@/pages/ide/observability/clusters";
import MetricDetailPage from "@/pages/ide/observability/metrics/MetricsDetailPage";
import MetricsPage from "@/pages/ide/observability/metrics/MetricsListPage";
import TraceDetailPage from "@/pages/ide/observability/trace";
import TracesPage from "@/pages/ide/observability/traces";
import ThreadPage from "@/pages/thread";
import Threads from "@/pages/threads";
import WorkflowPage from "@/pages/workflow";
import "@xyflow/react/dist/style.css";
import React, { useEffect, useRef } from "react";
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
import WorkspaceStatus from "./components/WorkspaceStatus";
import AgenticSetupPage from "./components/workspaces/components/CreateWorkspaceDialog/components/AgenticSetup";
import { LocalWorkspaceSetupDialog } from "./components/workspaces/components/LocalWorkspaceSetupDialog";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { useWorkspace } from "./hooks/api/workspaces/useWorkspaces";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import { LOCAL_WORKSPACE_ID } from "./libs/utils/constants";
import { setLastWorkspaceId } from "./libs/utils/lastWorkspace";
import AdminBillingQueue from "./pages/admin/AdminBillingQueue";
import AdminFeatureFlags from "./pages/admin/AdminFeatureFlags";
import AdminLayout from "./pages/admin/AdminLayout";
import AppPage from "./pages/app";
import GoogleCallback from "./pages/auth/GoogleCallback";
import MagicLinkCallback from "./pages/auth/MagicLinkCallback";
import OktaCallback from "./pages/auth/OktaCallback";
import CheckoutCancelledPage from "./pages/billing/CheckoutCancelled";
import CheckoutSuccessPage from "./pages/billing/CheckoutSuccess";
import GitHubCallback from "./pages/github/callback";
import InvitePage from "./pages/Invite";
import IdePage from "./pages/ide";
import CoordinatorLayout from "./pages/ide/coordinator";
import ActiveRunsPage from "./pages/ide/coordinator/ActiveRuns";
import QueueHealthPage from "./pages/ide/coordinator/QueueHealth";
import RecoveryPage from "./pages/ide/coordinator/Recovery";
import RunHistoryPage from "./pages/ide/coordinator/RunHistory";
import RunTreePage from "./pages/ide/coordinator/RunTree";
import DatabaseLayout from "./pages/ide/Database";
import QueryWorkspacePage from "./pages/ide/Database/QueryWorkspace";
import FilesLayout from "./pages/ide/Files";
import EditorPage from "./pages/ide/Files/Editor";
import LookerExplorerPage from "./pages/ide/Files/Editor/LookerExplore";
import ModelingPage from "./pages/ide/modeling";
import ObservabilityLayout from "./pages/ide/observability";
import ExecutionAnalytics from "./pages/ide/observability/execution-analytics";
import SettingsLayout from "./pages/ide/settings";
import ActivityLogsPage from "./pages/ide/settings/activity-logs";
import AirhouseSettingsPage from "./pages/ide/settings/airhouse";
import ApiKeysPage from "./pages/ide/settings/api-keys";
import DatabasesPage from "./pages/ide/settings/databases";
import WorkspaceMembersPage from "./pages/ide/settings/members";
import RepositoriesPage from "./pages/ide/settings/repositories";
import SecretsPage from "./pages/ide/settings/secrets";
import TestsLayout from "./pages/ide/tests";
import TestFileDetailPage from "./pages/ide/tests/TestFileDetailPage";
import TestsDashboardPage from "./pages/ide/tests/TestsDashboardPage";
import TestsRunsPage from "./pages/ide/tests/TestsRunsPage";
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

const MainPageWrapper = ({ children }: { children: React.ReactNode }) => {
  return (
    <main className='flex h-full w-full min-w-0 flex-col bg-background'>
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
          path='workflows/:pathb64'
          element={
            <MainPageWrapper>
              <WorkflowPage />
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
        <Route path='ide' element={<IdePage />}>
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

          {/* Settings routes */}
          <Route path='settings' element={<SettingsLayout />}>
            <Route path='databases' element={<DatabasesPage />} />
            <Route path='repositories' element={<RepositoriesPage />} />
            <Route path='activity-logs' element={<ActivityLogsPage />} />
            <Route path='api-keys' element={<ApiKeysPage />} />
            <Route path='secrets' element={<SecretsPage />} />
            <Route path='members' element={<WorkspaceMembersPage />} />
            <Route path='airhouse' element={<AirhouseSettingsPage />} />
          </Route>

          {/* Data Modeling routes */}
          <Route path='modeling' element={<ModelingPage />} />

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
          <Route element={<AdminLayout />}>
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
              <Route path='billing/checkout-success' element={<CheckoutSuccessPage />} />
              <Route path='billing/checkout-cancelled' element={<CheckoutCancelledPage />} />

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
