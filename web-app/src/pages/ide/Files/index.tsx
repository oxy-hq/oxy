import React, { useState } from "react";
import { Outlet, useParams } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import { ChevronsRight } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import EmptyState from "@/components/ui/EmptyState";
import FilesSidebar from "./FilesSidebar";
import { FilesProvider } from "./FilesContext";

const FilesLayout: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const { pathb64 } = useParams();
  const hasContent = !!pathb64;

  return (
    <FilesProvider>
      <ResizablePanelGroup direction="horizontal" className="flex-1">
        {sidebarOpen ? (
          <>
            <ResizablePanel
              defaultSize={20}
              minSize={10}
              className="min-w-[200px]"
            >
              <FilesSidebar setSidebarOpen={setSidebarOpen} />
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
        <ResizablePanel
          defaultSize={sidebarOpen ? 80 : 100}
          minSize={20}
          className="relative"
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
    </FilesProvider>
  );
};

export default FilesLayout;
