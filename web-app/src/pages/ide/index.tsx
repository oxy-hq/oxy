import { Outlet, useParams, useLocation } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import Sidebar from "./Sidebar";
import { createContext, useContext, useEffect, useRef, useState } from "react";
import { cn } from "@/libs/shadcn/utils";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import EmptyState from "@/components/ui/EmptyState";
import Header from "./Header";
import ProjectStatus from "@/components/ProjectStatus";

const IDEContext = createContext<{ insideIDE: boolean }>({ insideIDE: false });

// eslint-disable-next-line react-refresh/only-export-components
export const useIDE = () => {
  return useContext(IDEContext);
};

const Ide = () => {
  const { pathb64 } = useParams();
  const location = useLocation();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const { open, setOpen } = useSidebar();

  const hasClosedSidebar = useRef(false);

  const isObservabilityRoute = location.pathname.includes("/ide/observability");
  const hasContent = pathb64 || isObservabilityRoute;

  useEffect(() => {
    if (open && !hasClosedSidebar.current) {
      setOpen(false);
      hasClosedSidebar.current = true;
    }
  }, [open, setOpen]);

  return (
    <IDEContext.Provider value={{ insideIDE: true }}>
      <div className="flex h-full flex-1 overflow-hidden flex-col">
        <ProjectStatus />
        <Header />
        <div className="flex flex-1 overflow-hidden">
          <ResizablePanelGroup direction="horizontal">
            <ResizablePanel
              defaultSize={20}
              minSize={10}
              className={cn(!sidebarOpen && "flex-[unset]!")}
            >
              <Sidebar
                sidebarOpen={sidebarOpen}
                setSidebarOpen={setSidebarOpen}
              />
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel
              defaultSize={80}
              minSize={20}
              className={cn(!sidebarOpen && "flex-1!", "relative")}
            >
              {!hasContent ? (
                <EmptyState
                  title="No file is open"
                  description="Select a file from the sidebar to start editing"
                  className="absolute inset-0 mt-[-150px]"
                />
              ) : (
                <Outlet />
              )}
            </ResizablePanel>
          </ResizablePanelGroup>
        </div>
      </div>
    </IDEContext.Provider>
  );
};

export default Ide;
