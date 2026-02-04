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
import { SidebarProvider } from "@/components/ui/shadcn/sidebar";
import { Toaster as ShadcnToaster } from "@/components/ui/shadcn/sonner";
import CreateWorkspacePage from "@/pages/create-workspace";
import Home from "@/pages/home";
import ClusterMapPage from "@/pages/ide/observability/clusters";
import MetricDetailPage from "@/pages/ide/observability/metrics/MetricsDetailPage";
import MetricsPage from "@/pages/ide/observability/metrics/MetricsListPage";
import TraceDetailPage from "@/pages/ide/observability/trace";
import TracesPage from "@/pages/ide/observability/traces";
import ThreadPage from "@/pages/thread";
import Threads from "@/pages/threads";
import WorkflowPage from "@/pages/workflow";
import WorkspacesPage from "@/pages/workspaces";
import "@xyflow/react/dist/style.css";
import { Loader2 } from "lucide-react";
import React from "react";
import { HotkeysProvider } from "react-hotkeys-hook";
import ROUTES from "@/libs/utils/routes";
import OntologyPage from "@/pages/ontology";
import { ErrorBoundary } from "@/sentry";
import ProjectStatus from "./components/ProjectStatus";
import ProtectedRoute from "./components/ProtectedRoute";
import { SettingsModal } from "./components/settings/SettingsModal";
import RequiredSecretsSetup from "./components/settings/secrets/RequiredSecretsSetup";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { useProject } from "./hooks/api/projects/useProjects";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import AppPage from "./pages/app";
import EmailVerificationPage from "./pages/auth/EmailVerification";
import GoogleCallback from "./pages/auth/GoogleCallback";
import OktaCallback from "./pages/auth/OktaCallback";
import GitHubCallback from "./pages/github/callback";
import IdePage from "./pages/ide";
import DatabaseLayout from "./pages/ide/Database";
import QueryWorkspacePage from "./pages/ide/Database/QueryWorkspace";
import FilesLayout from "./pages/ide/Files";
import EditorPage from "./pages/ide/Files/Editor";
import ObservabilityLayout from "./pages/ide/observability";
import ExecutionAnalytics from "./pages/ide/observability/execution-analytics";
import SettingsLayout from "./pages/ide/settings";
import ActivityLogsPage from "./pages/ide/settings/activity-logs";
import DatabasesPage from "./pages/ide/settings/databases";
import LoginPage from "./pages/login";
import RegisterPage from "./pages/register";
import useCurrentProject from "./stores/useCurrentProject";
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

  React.useEffect(() => {
    if (!isPending && !isError && data) {
      setProject(data);
    }
  }, [isPending, isError, setProject, data]);

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin' />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-red-600'>Failed to load project.</p>
      </div>
    );
  }

  if (!project) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-red-600'>Project not found.</p>
      </div>
    );
  }

  return (
    <HotkeysProvider>
      <SettingsModal />
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
          </Route>

          {/* Database routes */}
          <Route path='database' element={<DatabaseLayout />}>
            <Route index element={<QueryWorkspacePage />} />
          </Route>

          {/* Settings routes */}
          <Route path='settings' element={<SettingsLayout />}>
            <Route path='databases' element={<DatabasesPage />} />
            <Route path='activity-logs' element={<ActivityLogsPage />} />
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
          path='/ontology'
          element={
            <MainPageWrapper>
              <OntologyPage />
            </MainPageWrapper>
          }
        />

        {!authConfig.cloud && <Route path='*' element={<Navigate to='/' />} />}
      </Routes>
    </HotkeysProvider>
  );
});

const getLocalRouter = (authConfig: AuthConfigResponse) =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route>
        {/* Auth routes for non-cloud mode when auth is enabled */}
        {authConfig.is_built_in_mode && authConfig.auth_enabled && (
          <>
            <Route path={ROUTES.AUTH.LOGIN} element={<LoginPage />} />
            <Route path={ROUTES.AUTH.REGISTER} element={<RegisterPage />} />
            <Route path={ROUTES.AUTH.VERIFY_EMAIL} element={<EmailVerificationPage />} />
            <Route path={ROUTES.AUTH.GOOGLE_CALLBACK} element={<GoogleCallback />} />
            <Route path={ROUTES.AUTH.OKTA_CALLBACK} element={<OktaCallback />} />
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

const getRouter = (authConfig: AuthConfigResponse) =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route>
        {authConfig.is_built_in_mode && (
          <>
            <Route path={ROUTES.AUTH.LOGIN} element={<LoginPage />} />
            <Route path={ROUTES.AUTH.REGISTER} element={<RegisterPage />} />
            <Route path={ROUTES.AUTH.VERIFY_EMAIL} element={<EmailVerificationPage />} />
            <Route path={ROUTES.AUTH.GOOGLE_CALLBACK} element={<GoogleCallback />} />
            <Route path={ROUTES.AUTH.OKTA_CALLBACK} element={<OktaCallback />} />
          </>
        )}

        {/* GitHub callback route for handling app installations */}
        <Route
          path={ROUTES.GITHUB.CALLBACK}
          element={
            <ProtectedRoute>
              <GitHubCallback />
            </ProtectedRoute>
          }
        />

        <Route
          path={ROUTES.WORKSPACE.ROOT}
          element={
            <ProtectedRoute>
              <WorkspacesPage />
            </ProtectedRoute>
          }
        />

        <Route
          path={ROUTES.WORKSPACE.CREATE_WORKSPACE}
          element={
            <ProtectedRoute>
              <CreateWorkspacePage />
            </ProtectedRoute>
          }
        />

        <Route path='/projects/:projectId/settings/secrets' element={<RequiredSecretsSetup />} />

        <Route
          path='/projects/:projectId/*'
          element={
            <ProtectedRoute>
              <SidebarProvider>
                <MainLayout />
              </SidebarProvider>
            </ProtectedRoute>
          }
        />

        <Route path='*' element={<Navigate to={ROUTES.WORKSPACE.ROOT} />} />

        <Route
          path={ROUTES.ROOT}
          element={
            <ProtectedRoute>
              <Navigate to={ROUTES.WORKSPACE.ROOT} replace />
            </ProtectedRoute>
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
        <Loader2 className='h-4 w-4 animate-spin' />
      </div>
    );
  }

  return (
    <ErrorBoundary fallback={<div>Something went wrong. Please refresh.</div>} showDialog>
      <AuthProvider authConfig={authConfig}>
        <RouterProvider
          router={authConfig.cloud ? getRouter(authConfig) : getLocalRouter(authConfig)}
        />
        <ShadcnToaster />
      </AuthProvider>
    </ErrorBoundary>
  );
}

export default App;
