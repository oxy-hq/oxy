import "@/styles/shadcn/index.css";
import {
  Navigate,
  Route,
  BrowserRouter as Router,
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
import AgentTestsPage from "@/pages/agent-test";
import NotSignedIn from "@/pages/NotSignedIn";
import React from "react";

const MainLayout = React.memo(function MainLayout() {
  return (
    <>
      <AppSidebar />
      <main className="bg-background w-full rounded-xl my-2 mr-2 shadow-[0px_1px_3px_0px_rgba(0,0,0,0.10),0px_1px_2px_0px_rgba(0,0,0,0.06)]">
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/threads" element={<Threads />} />
          <Route path="/threads/:threadId" element={<ThreadPage />} />
          <Route path="/workflows/:pathb64" element={<WorkflowPage />} />
          <Route path="/agents/:pathb64/tests" element={<AgentTestsPage />} />
          <Route path="*" element={<Navigate to="/" />} />
        </Routes>
      </main>
    </>
  );
});

function App() {
  return (
    <Router>
      <SidebarProvider>
        <Routes>
          <Route path="/not_signed_in" element={<NotSignedIn />} />
          <Route path="*" element={<MainLayout />} />
        </Routes>
      </SidebarProvider>
      <ShadcnToaster />
    </Router>
  );
}

export default App;
