import "@/styles/shadcn/index.css";
import {
  Navigate,
  Route,
  createBrowserRouter,
  createRoutesFromElements,
  RouterProvider,
  Routes,
  useParams,
} from "react-router-dom";
import Home from "@/pages/home";
import WorkspacesPage from "@/pages/workspaces";
import CreateWorkspacePage from "@/pages/create-workspace";
import { AppSidebar } from "@/components/AppSidebar";
import { Toaster as ShadcnToaster } from "@/components/ui/shadcn/sonner";
import { SidebarProvider } from "@/components/ui/shadcn/sidebar";
import Threads from "@/pages/threads";
import ThreadPage from "@/pages/thread";
import WorkflowPage from "@/pages/workflow";
import "@xyflow/react/dist/style.css";
import React from "react";
import IdePage from "./pages/ide";
import EditorPage from "./pages/ide/Editor";
import AppPage from "./pages/app";
import { HotkeysProvider } from "react-hotkeys-hook";
import LoginPage from "./pages/login";
import RegisterPage from "./pages/register";
import EmailVerificationPage from "./pages/auth/EmailVerification";
import GoogleCallback from "./pages/auth/GoogleCallback";
import GitHubCallback from "./pages/github/callback";
import ProtectedRoute from "./components/ProtectedRoute";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import { Loader2 } from "lucide-react";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { AuthConfigResponse } from "./types/auth";
import { SettingsModal } from "./components/settings/SettingsModal";
import ROUTES from "@/libs/utils/routes";
import RequiredSecretsSetup from "./components/settings/secrets/RequiredSecretsSetup";
import useCurrentProject from "./stores/useCurrentProject";
import { useProject } from "./hooks/api/projects/useProjects";
import ProjectStatus from "./components/ProjectStatus";
import { ErrorBoundary } from "@/sentry";

const MainPageWrapper = ({ children }: { children: React.ReactNode }) => {
  return (
    <main className="bg-background w-full h-full min-w-0 flex flex-col">
      <ProjectStatus />
      <div className="flex-1 w-full min-w-0 overflow-hidden">{children}</div>
    </main>
  );
};

const MainLayout = React.memo(function MainLayout() {
  const { authConfig } = useAuth();
  const { projectId } = useParams();

  const { isPending, isError, data } = useProject(
    projectId || "",
    !!authConfig.cloud,
  );
  const { setProject, project } = useCurrentProject();

  React.useEffect(() => {
    if (!isPending && !isError && data) {
      setProject(data);
    }
  }, [isPending, isError, projectId, setProject, data]);

  if (isPending) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <Loader2 className="animate-spin h-4 w-4" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <p className="text-red-600">Failed to load project.</p>
      </div>
    );
  }

  if (!project) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <p className="text-red-600">Project not found.</p>
      </div>
    );
  }

  return (
    <HotkeysProvider>
      <SettingsModal />
      <AppSidebar />

      <Routes>
        <Route
          path="/"
          element={
            <MainPageWrapper>
              <Home />
            </MainPageWrapper>
          }
        />

        <Route
          path="/home"
          element={
            <MainPageWrapper>
              <Home />
            </MainPageWrapper>
          }
        />
        <Route
          path="/threads"
          element={
            <MainPageWrapper>
              <Threads />
            </MainPageWrapper>
          }
        />
        <Route
          path="/threads/:threadId"
          element={
            <MainPageWrapper>
              <ThreadPage />
            </MainPageWrapper>
          }
        />
        <Route
          path="/workflows/:pathb64"
          element={
            <MainPageWrapper>
              <WorkflowPage />
            </MainPageWrapper>
          }
        />
        <Route
          path="/apps/:pathb64"
          element={
            <MainPageWrapper>
              <AppPage />
            </MainPageWrapper>
          }
        />
        <Route path="/ide" element={<IdePage />}>
          <Route path=":pathb64" element={<EditorPage />} />
        </Route>

        {!authConfig.cloud && <Route path="*" element={<Navigate to="/" />} />}
      </Routes>
    </HotkeysProvider>
  );
});

const getLocalRouter = () =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route>
        <Route
          path="/*"
          element={
            <SidebarProvider>
              <MainLayout />
            </SidebarProvider>
          }
        />
      </Route>,
    ),
  );

const getRouter = (authConfig: AuthConfigResponse) =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route>
        {authConfig.is_built_in_mode && (
          <>
            <Route path={ROUTES.AUTH.LOGIN} element={<LoginPage />} />
            <Route path={ROUTES.AUTH.REGISTER} element={<RegisterPage />} />
            <Route
              path={ROUTES.AUTH.VERIFY_EMAIL}
              element={<EmailVerificationPage />}
            />
            <Route
              path={ROUTES.AUTH.GOOGLE_CALLBACK}
              element={<GoogleCallback />}
            />
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

        <Route
          path="/projects/:projectId/settings/secrets"
          element={<RequiredSecretsSetup />}
        />

        <Route
          path="/projects/:projectId/*"
          element={
            <ProtectedRoute>
              <SidebarProvider>
                <MainLayout />
              </SidebarProvider>
            </ProtectedRoute>
          }
        />

        <Route path="*" element={<Navigate to={ROUTES.WORKSPACE.ROOT} />} />

        <Route
          path={ROUTES.ROOT}
          element={
            <ProtectedRoute>
              <Navigate to={ROUTES.WORKSPACE.ROOT} replace />
            </ProtectedRoute>
          }
        />
      </Route>,
    ),
  );

function App() {
  const { data: authConfig, isPending } = useAuthConfig();

  if (isPending || !authConfig) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <Loader2 className="animate-spin h-4 w-4" />
      </div>
    );
  }

  return (
    <ErrorBoundary
      fallback={<div>Something went wrong. Please refresh.</div>}
      showDialog
    >
      <AuthProvider authConfig={authConfig}>
        <RouterProvider
          router={authConfig.cloud ? getRouter(authConfig) : getLocalRouter()}
        />
        <ShadcnToaster />
      </AuthProvider>
    </ErrorBoundary>
  );
}

export default App;
