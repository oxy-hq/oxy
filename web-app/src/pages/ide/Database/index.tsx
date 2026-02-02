import React, { useState } from "react";
import { Outlet } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import { ChevronsRight } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { DatabaseSidebar } from "./components";

const DatabaseLayout: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);

  return (
    <ResizablePanelGroup direction="horizontal" className="flex-1">
      {sidebarOpen ? (
        <>
          <ResizablePanel defaultSize={20} minSize={10} className="min-w-[200px]">
            <DatabaseSidebar sidebarOpen={sidebarOpen} setSidebarOpen={setSidebarOpen} />
          </ResizablePanel>
          <ResizableHandle />
        </>
      ) : (
        <div className="border-r bg-sidebar-background flex items-start py-2 px-1">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setSidebarOpen(true)}
            tooltip={{ content: "Expand Sidebar", side: "right" }}
            className="h-8 w-8"
          >
            <ChevronsRight className="h-4 w-4" />
          </Button>
        </div>
      )}
      <ResizablePanel defaultSize={sidebarOpen ? 80 : 100} minSize={20}>
        <Outlet />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
};

export default DatabaseLayout;
