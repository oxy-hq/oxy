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
import NotSignedIn from "@/pages/NotSignedIn";
import React from "react";
import IdePage from "./pages/ide";
import EditorPage from "./pages/ide/Editor";
import AppPage from "./pages/app";
import { HotkeysProvider } from "react-hotkeys-hook";
import TaskPage from "./pages/task/index.tsx";

const PageWrapper = ({ children }: { children: React.ReactNode }) => {
  return <main className="bg-background w-full h-full">{children}</main>;
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
          path="/tasks/:taskId"
          element={
            <PageWrapper>
              <TaskPage />
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
        <Route path="*" element={<Navigate to="/" />} />
      </Routes>
    </HotkeysProvider>
  );
});

const router = createBrowserRouter(
  createRoutesFromElements(
    <Route>
      <Route path="/not_signed_in" element={<NotSignedIn />} />
      <Route
        path="*"
        element={
          <SidebarProvider>
            <MainLayout />
          </SidebarProvider>
        }
      />
    </Route>,
  ),
);

function App() {
  return (
    <>
      <RouterProvider router={router} />
      <ShadcnToaster />
    </>
  );
}

export default App;
