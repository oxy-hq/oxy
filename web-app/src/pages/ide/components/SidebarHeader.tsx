import React from "react";
import { ChevronsLeft } from "lucide-react";
import { SidebarGroupLabel } from "@/components/ui/shadcn/sidebar";
import { Button } from "@/components/ui/shadcn/button";

interface SidebarHeaderProps {
  title: string;
  onCollapse: () => void;
  actions?: React.ReactNode;
}

/**
 * A shared header component for IDE sidebars (Database, Files, Observability, Settings).
 * Provides consistent styling for the title, optional action buttons, and collapse button.
 */
export const SidebarHeader: React.FC<SidebarHeaderProps> = ({
  title,
  onCollapse,
  actions,
}) => {
  return (
    <SidebarGroupLabel className="h-auto flex items-center justify-between px-2 py-1 border-b border-sidebar-border rounded-none min-h-[41px]">
      <span className="text-sm font-semibold">{title}</span>
      <div className="flex items-center gap-0.5">
        {actions}
        <Button
          className="md:hidden"
          variant="ghost"
          size="sm"
          onClick={onCollapse}
          tooltip="Collapse Sidebar"
        >
          <ChevronsLeft className="h-4 w-4" />
        </Button>
      </div>
    </SidebarGroupLabel>
  );
};

export default SidebarHeader;
