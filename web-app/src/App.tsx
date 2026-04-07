import "@/styles/shadcn/index.css";
import {
  createBrowserRouter,
  createRoutesFromElements,
  Navigate,
  Route,
  RouterProvider,
  Routes,
  useParams
} from "react-router-dom";
import { AppSidebar } from "@/components/AppSidebar";
import ErrorAlert from "@/components/ui/ErrorAlert";
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
import React from "react";
import { HotkeysProvider, useHotkeys } from "react-hotkeys-hook";
import { Spinner } from "@/components/ui/shadcn/spinner";
import ROUTES from "@/libs/utils/routes";
import ContextGraphPage from "@/pages/context-graph";
import { ErrorBoundary } from "@/sentry";
import { BuilderDialog } from "./components/BuilderDialog";
import { FileQuickOpen } from "./components/FileQuickOpen";
import ProjectStatus from "./components/ProjectStatus";
import ProtectedRoute from "./components/ProtectedRoute";
import { SettingsModal } from "./components/settings/SettingsModal";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { useProject } from "./hooks/api/projects/useProjects";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import AppPage from "./pages/app";
import GoogleCallback from "./pages/auth/GoogleCallback";
import MagicLinkCallback from "./pages/auth/MagicLinkCallback";
import OktaCallback from "./pages/auth/OktaCallback";
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
import SecretsPage from "./pages/ide/settings/secrets";
import TestsLayout from "./pages/ide/tests";
import TestFileDetailPage from "./pages/ide/tests/TestFileDetailPage";
import TestsDashboardPage from "./pages/ide/tests/TestsDashboardPage";
import TestsRunsPage from "./pages/ide/tests/TestsRunsPage";
import LoginPage from "./pages/login";
import useBuilderDialog from "./stores/useBuilderDialog";
import useCurrentProject from "./stores/useCurrentProject";
import useFileQuickOpen from "./stores/useFileQuickOpen";
import type { AuthConfigResponse } from "./types/auth";

const MainPageWrapper = ({ children }: { children: React.ReactNode }) => {
  return (
    <main className='flex h-full w-full min-w-0 flex-col bg-background'>
      <ProjectStatus />
      <div className='w-full min-w-0 flex-1 overflow-hidden'>{children}</div>
    </main>
  );
};

const MainLayout = React.memo(function MainLayout() {
  const { authConfig } = useAuth();
  const { projectId } = useParams();

  const { isPending, isError, data } = useProject(projectId || "", !!authConfig.cloud);
  const { setProject, project } = useCurrentProject();

  const { setIsOpen: setBuilderDialogOpen } = useBuilderDialog();
  const { setIsOpen: setFileQuickOpenOpen } = useFileQuickOpen();
  useHotkeys("meta+i", () => setBuilderDialogOpen(true), { preventDefault: true });
  useHotkeys("meta+p", () => setFileQuickOpenOpen(true), { preventDefault: true });

  // biome-ignore lint/correctness/useExhaustiveDependencies: project hydration is intentionally gated by the fetched project payload
  React.useEffect(() => {
    if (!isPending && !isError && data) {
      setProject(data);
    }
  }, [isPending, isError, projectId, setProject, data]);

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full w-full items-center justify-center p-4'>
        <ErrorAlert message='Failed to load project.' />
      </div>
    );
  }

  if (!project) {
    return (
      <div className='flex h-full w-full items-center justify-center p-4'>
        <ErrorAlert message='Project not found.' />
      </div>
    );
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
        <Route
          path='/workflows/:pathb64'
          element={
            <MainPageWrapper>
              <WorkflowPage />
            </MainPageWrapper>
          }
        />
        <Route
          path='/apps/:pathb64'
          element={
            <MainPageWrapper>
              <AppPage />
            </MainPageWrapper>
          }
        />
        <Route path='/ide' element={<IdePage />}>
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
          path='/context-graph'
          element={
            <MainPageWrapper>
              <ContextGraphPage />
            </MainPageWrapper>
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
      </Route>
    )
  );

function App() {
  const { data: authConfig, isPending } = useAuthConfig();

  if (isPending || !authConfig) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner />
      </div>
    );
  }

  return (
    <ErrorBoundary fallback={<div>Something went wrong. Please refresh.</div>} showDialog>
      <AuthProvider authConfig={authConfig}>
        <RouterProvider router={getRouter(authConfig)} />
        <ShadcnToaster />
      </AuthProvider>
    </ErrorBoundary>
  );
}

export default App;
