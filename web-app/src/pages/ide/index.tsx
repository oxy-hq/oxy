import { Outlet } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import Sidebar from "./Sidebar";
import { useState } from "react";
import { cn } from "@/libs/shadcn/utils";
const Ide = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);

  return (
    <div className="flex h-full flex-1 overflow-hidden">
      <ResizablePanelGroup direction="horizontal">
        <ResizablePanel
          defaultSize={20}
          minSize={10}
          className={cn(!sidebarOpen && "flex-[unset]!")}
        >
          <Sidebar sidebarOpen={sidebarOpen} setSidebarOpen={setSidebarOpen} />
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel
          defaultSize={80}
          minSize={20}
          className={cn(!sidebarOpen && "flex-1!")}
        >
          <Outlet />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
};

export default Ide;
