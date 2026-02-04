import { ChevronsRight } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Outlet, useParams } from "react-router-dom";
import EmptyState from "@/components/ui/EmptyState";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { FilesProvider } from "./FilesContext";
import FilesSidebar from "./FilesSidebar";

const FilesLayout: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const { pathb64 } = useParams();
  const hasContent = !!pathb64;

  return (
    <FilesProvider>
      <ResizablePanelGroup direction='horizontal' className='flex-1'>
        {sidebarOpen ? (
          <>
            <ResizablePanel defaultSize={20} minSize={10} className='min-w-[200px]'>
              <FilesSidebar setSidebarOpen={setSidebarOpen} />
            </ResizablePanel>
            <ResizableHandle />
          </>
        ) : (
          <div className='flex items-start border-r bg-sidebar-background px-1 py-2'>
            <Button
              variant='ghost'
              size='icon'
              onClick={() => setSidebarOpen(true)}
              tooltip={{ content: "Expand Sidebar", side: "right" }}
              className='h-8 w-8'
            >
              <ChevronsRight className='h-4 w-4' />
            </Button>
          </div>
        )}
        <ResizablePanel defaultSize={sidebarOpen ? 80 : 100} minSize={20} className='relative'>
          {!hasContent ? (
            <EmptyState
              title='No file is open'
              description='Select a file from the sidebar to start editing'
              className='absolute inset-0 mt-[-150px]'
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
