import "@/styles/shadcn/index.css";
import {
  createBrowserRouter,
  createRoutesFromElements,
  Navigate,
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
import ProtectedRoute from "./components/ProtectedRoute";
import { SettingsModal } from "./components/settings/SettingsModal";
import WorkspaceStatus from "./components/WorkspaceStatus";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { useWorkspace } from "./hooks/api/workspaces/useWorkspaces";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import AppPage from "./pages/app";
import GoogleCallback from "./pages/auth/GoogleCallback";
import MagicLinkCallback from "./pages/auth/MagicLinkCallback";
import OktaCallback from "./pages/auth/OktaCallback";
import GitHubCallback from "./pages/github/callback";
import IdePage from "./pages/ide";
import DatabaseLayout from "./pages/ide/Database";
import QueryWorkspacePage from "./pages/ide/Database/QueryWorkspace";
import FilesLayout from "./pages/ide/Files";
import EditorPage from "./pages/ide/Files/Editor";
import LookerExplorerPage from "./pages/ide/Files/Editor/LookerExplore";
import ObservabilityLayout from "./pages/ide/observability";
import ExecutionAnalytics from "./pages/ide/observability/execution-analytics";
import SettingsLayout from "./pages/ide/settings";
import ActivityLogsPage from "./pages/ide/settings/activity-logs";
import ApiKeysPage from "./pages/ide/settings/api-keys";
import DatabasesPage from "./pages/ide/settings/databases";
import RepositoriesPage from "./pages/ide/settings/repositories";
import SecretsPage from "./pages/ide/settings/secrets";
import TestsLayout from "./pages/ide/tests";
import TestFileDetailPage from "./pages/ide/tests/TestFileDetailPage";
import TestsDashboardPage from "./pages/ide/tests/TestsDashboardPage";
import TestsRunsPage from "./pages/ide/tests/TestsRunsPage";
import LoginPage from "./pages/login";
import MembersPage from "./pages/members";
import OnboardingPage from "./pages/onboarding";
import WorkspacesPage from "./pages/workspaces";
import useBuilderDialog from "./stores/useBuilderDialog";
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

const MainLayout = React.memo(function MainLayout() {
  const { authConfig } = useAuth();
  const { workspaceId } = useParams();
  const navigate = useNavigate();

  const { isPending, isError, error, data } = useWorkspace(
    workspaceId || "00000000-0000-0000-0000-000000000000"
  );
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

  useEffect(() => {
    if (isError) {
      const msg =
        (error as { response?: { data?: { error?: string } } })?.response?.data?.error ??
        "Failed to load workspace.";
      toast.error(msg);
      navigate(ROUTES.WORKSPACES, { replace: true });
    }
  }, [isError, error, navigate]);

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner />
      </div>
    );
  }

  if (isError || !workspace) {
    return null;
  }

  return (
    <HotkeysProvider>
      <SettingsModal />
      <BuilderDialog />
      <FileQuickOpen />
      <AppSidebar />

      <Routes>
        <Route
          path='/'
          element={
            <MainPageWrapper>
              <Home />
            </MainPageWrapper>
          }
        />

        <Route
          path='/home'
          element={
            <MainPageWrapper>
              <Home />
            </MainPageWrapper>
          }
        />
        <Route
          path='/threads'
          element={
            <MainPageWrapper>
              <Threads />
            </MainPageWrapper>
          }
        />
        <Route
          path='/threads/:threadId'
          element={
            <MainPageWrapper>
              <ThreadPage />
            </MainPageWrapper>
          }
        />
        {/* Workspace-scoped deep-link routes — used for shareable thread/workflow URLs */}
        <Route
          path='/workspaces/:workspaceId/threads/:threadId'
          element={
            <MainPageWrapper>
              <ThreadPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='/workflows/:pathb64'
          element={
            <MainPageWrapper>
              <WorkflowPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='/workspaces/:workspaceId/workflows/:pathb64'
          element={
            <MainPageWrapper>
              <WorkflowPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='/workspaces/:workspaceId/apps/:pathb64'
          element={
            <MainPageWrapper>
              <AppPage />
            </MainPageWrapper>
          }
        />
        {/* IDE is workspace-scoped — URL encodes the workspace so bookmarks/links always open the right workspace */}
        <Route path='/workspaces/:workspaceId/ide' element={<IdePage />}>
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
          </Route>

          {/* Tests routes */}
          <Route path='tests' element={<TestsLayout />}>
            <Route index element={<TestsDashboardPage />} />
            <Route path='runs' element={<TestsRunsPage />} />
            <Route path=':pathb64' element={<TestFileDetailPage />} />
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
          path='/workspaces/:workspaceId/context-graph'
          element={
            <MainPageWrapper>
              <ContextGraphPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='/workspaces'
          element={
            <MainPageWrapper>
              <WorkspacesPage />
            </MainPageWrapper>
          }
        />

        <Route
          path='/members'
          element={
            !authConfig.auth_enabled || authConfig.single_workspace ? (
              <Navigate to='/' replace />
            ) : (
              <MainPageWrapper>
                <MembersPage />
              </MainPageWrapper>
            )
          }
        />

        <Route path='*' element={<Navigate to='/' />} />
      </Routes>
    </HotkeysProvider>
  );
});

const getRouter = (authConfig: AuthConfigResponse) =>
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

        {/* GitHub callback must always be accessible (used during onboarding popup flow) */}
        <Route path='/github/callback' element={<GitHubCallback />} />

        {/* Setup page — always accessible as a standalone full-screen page (no sidebar) */}
        <Route path='/setup' element={<OnboardingPage />} />

        {authConfig.needs_onboarding ? (
          <Route path='*' element={<Navigate to='/setup' replace />} />
        ) : (
          <Route
            path='/*'
            element={
              authConfig.auth_enabled ? (
                <ProtectedRoute>
                  <SidebarProvider>
                    <MainLayout />
                  </SidebarProvider>
                </ProtectedRoute>
              ) : (
                <SidebarProvider>
                  <MainLayout />
                </SidebarProvider>
              )
            }
          />
        )}
      </Route>
    )
  );

function App() {
  const { data: authConfig, isPending } = useAuthConfig();

  // Show a one-time toast when the server reports a workspace error (e.g. the
  // previously active workspace directory was deleted since last run).
  const shownWorkspaceError = useRef(false);
  useEffect(() => {
    if (authConfig?.workspace_error && !shownWorkspaceError.current) {
      shownWorkspaceError.current = true;
      toast.error(authConfig.workspace_error);
    }
  }, [authConfig?.workspace_error]);

  // Only recreate the router when routing-relevant fields change — prevents the
  // router from being torn down on every authConfig refetch (e.g. when a GitHub
  // popup closes and the window regains focus), which would reset page state.
  const routerRef = useRef<ReturnType<typeof getRouter> | null>(null);
  const prevRouterKey = useRef<string | null>(null);
  const routerKey = authConfig
    ? `${authConfig.needs_onboarding}:${authConfig.auth_enabled}:${authConfig.single_workspace}`
    : null;
  if (authConfig && routerKey !== prevRouterKey.current) {
    routerRef.current = getRouter(authConfig);
    prevRouterKey.current = routerKey;
  }
  const router = routerRef.current;

  if (isPending || !authConfig || !router) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner />
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
