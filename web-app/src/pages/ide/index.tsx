import { Outlet } from "react-router-dom";
import Sidebar from "./Sidebar";
import { createContext, useContext, useEffect, useRef } from "react";
import Header from "./Header";
import ProjectStatus from "@/components/ProjectStatus";
import useSidebar from "@/components/ui/shadcn/sidebar-context";

const IDEContext = createContext<{
  insideIDE: boolean;
}>({
  insideIDE: false,
});
export const useIDE = () => {
  return useContext(IDEContext);
};

const Ide = () => {
  const { open, setOpen } = useSidebar();

  const hasClosedSidebar = useRef(false);

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
          <Sidebar />
          <Outlet />
        </div>
      </div>
    </IDEContext.Provider>
  );
};

export default Ide;
