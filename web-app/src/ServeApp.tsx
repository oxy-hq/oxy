import "@/styles/shadcn/index.css";
import {
  Navigate,
  Route,
  BrowserRouter as Router,
  Routes,
} from "react-router-dom";

import Home from "@/pages/serve/home";
import { AppSidebar } from "@/components/serve/AppSidebar";
import { SidebarProvider } from "./components/ui/shadcn/sidebar";
import NewThread from "./pages/serve/new";
import Threads from "./pages/serve/threads";
import Thread from "./pages/serve/thread";

function ServeApp() {
  return (
    <Router>
      <SidebarProvider>
        <AppSidebar />
        <main className="bg-background w-full rounded-xl my-2 mr-2 shadow-[0px_1px_3px_0px_rgba(0,0,0,0.10),0px_1px_2px_0px_rgba(0,0,0,0.06)]">
          <Routes>
            <Route>
              <Route path="/" element={<Home />} />
              <Route path="/threads" element={<Threads />} />
              <Route path="/new" element={<NewThread />} />
              <Route path="/threads/:threadId" element={<Thread />} />
            </Route>
            <Route path="*" element={<Navigate to="/" />} />
          </Routes>
        </main>
      </SidebarProvider>
    </Router>
  );
}

export default ServeApp;
