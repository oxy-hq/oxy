import "@/styles/shadcn/index.css";
import {
  Navigate,
  Route,
  createBrowserRouter,
  createRoutesFromElements,
  RouterProvider,
  Routes,
} from "react-router-dom";
import Home from "@/pages/home";
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
import ApiKeyManagement from "./pages/api-keys";
import DatabaseManagement from "./pages/databases";
import { HotkeysProvider } from "react-hotkeys-hook";
import LoginPage from "./pages/login";
import RegisterPage from "./pages/register";
import EmailVerificationPage from "./pages/auth/EmailVerification";
import GoogleCallback from "./pages/auth/GoogleCallback";
import ProtectedRoute from "./components/ProtectedRoute";
import useAuthConfig from "./hooks/auth/useAuthConfig";
import { Loader2 } from "lucide-react";
import { AuthProvider } from "./contexts/AuthContext";
import { AuthConfigResponse } from "./types/auth";

const PageWrapper = ({ children }: { children: React.ReactNode }) => {
  return (
    <main className="bg-background w-full h-full min-w-0">{children}</main>
  );
};

const MainLayout = React.memo(function MainLayout() {
  return (
    <HotkeysProvider>
      <AppSidebar />

      <Routes>
        <Route
          path="/"
          element={
            <PageWrapper>
              <Home />
            </PageWrapper>
          }
        />
        <Route
          path="/threads"
          element={
            <PageWrapper>
              <Threads />
            </PageWrapper>
          }
        />
        <Route
          path="/threads/:threadId"
          element={
            <PageWrapper>
              <ThreadPage />
            </PageWrapper>
          }
        />
        <Route
          path="/workflows/:pathb64"
          element={
            <PageWrapper>
              <WorkflowPage />
            </PageWrapper>
          }
        />
        <Route
          path="/apps/:pathb64"
          element={
            <PageWrapper>
              <AppPage />
            </PageWrapper>
          }
        />
        <Route path="/ide" element={<IdePage />}>
          <Route path=":pathb64" element={<EditorPage />} />
        </Route>
        <Route
          path="/api-keys"
          element={
            <PageWrapper>
              <ApiKeyManagement />
            </PageWrapper>
          }
        />
        <Route
          path="/databases"
          element={
            <PageWrapper>
              <DatabaseManagement />
            </PageWrapper>
          }
        />
        <Route path="*" element={<Navigate to="/" />} />
      </Routes>
    </HotkeysProvider>
  );
});

const getRouter = (authConfig: AuthConfigResponse) =>
  createBrowserRouter(
    createRoutesFromElements(
      <Route>
        {authConfig.is_built_in_mode && (
          <>
            <Route path="/login" element={<LoginPage />} />
            <Route path="/register" element={<RegisterPage />} />
            <Route path="/verify-email" element={<EmailVerificationPage />} />
            <Route path="/auth/google/callback" element={<GoogleCallback />} />
          </>
        )}

        <Route
          path="*"
          element={
            <ProtectedRoute>
              <SidebarProvider>
                <MainLayout />
              </SidebarProvider>
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
    <AuthProvider authConfig={authConfig}>
      <RouterProvider router={getRouter(authConfig)} />
      <ShadcnToaster />
    </AuthProvider>
  );
}

export default App;
