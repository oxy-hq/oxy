import React from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { Folder, Activity, Settings, Database } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";

enum SidebarViewMode {
  FILES = "files",
  OBSERVABILITY = "observability",
  DATABASE = "database",
  SETTINGS = "settings",
}

const getViewModeFromPath = (pathname: string): SidebarViewMode => {
  if (pathname.includes("/ide/observability")) {
    return SidebarViewMode.OBSERVABILITY;
  }
  if (pathname.includes("/ide/database")) {
    return SidebarViewMode.DATABASE;
  }
  if (pathname.includes("/ide/settings")) {
    return SidebarViewMode.SETTINGS;
  }
  return SidebarViewMode.FILES;
};

const Sidebar: React.FC = () => {
  const navigate = useNavigate();
  const location = useLocation();
  const { authConfig } = useAuth();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const currentViewMode = getViewModeFromPath(location.pathname);

  const handleNavigate = (mode: SidebarViewMode) => {
    switch (mode) {
      case SidebarViewMode.FILES:
        navigate(ROUTES.PROJECT(projectId).IDE.FILES.ROOT);
        break;
      case SidebarViewMode.OBSERVABILITY:
        navigate(ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.TRACES);
        break;
      case SidebarViewMode.DATABASE:
        navigate(ROUTES.PROJECT(projectId).IDE.DATABASE.ROOT);
        break;
      case SidebarViewMode.SETTINGS:
        navigate(ROUTES.PROJECT(projectId).IDE.SETTINGS.DATABASES);
        break;
    }
  };

  return (
    <div className="h-full border-r bg-sidebar-background flex flex-col">
      <div className="flex flex-col items-center py-2 px-1 gap-1">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => handleNavigate(SidebarViewMode.FILES)}
          tooltip={{ content: "Files", side: "right" }}
          className={cn(
            "h-8 w-8",
            currentViewMode === SidebarViewMode.FILES &&
              "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
        >
          <Folder className="h-4 w-4" />
        </Button>

        {authConfig.enterprise && (
          <Button
            variant="ghost"
            size="icon"
            onClick={() => handleNavigate(SidebarViewMode.OBSERVABILITY)}
            tooltip={{ content: "Observability", side: "right" }}
            className={cn(
              "h-8 w-8",
              currentViewMode === SidebarViewMode.OBSERVABILITY &&
                "bg-sidebar-accent text-sidebar-accent-foreground",
            )}
          >
            <Activity className="h-4 w-4" />
          </Button>
        )}

        <Button
          variant="ghost"
          size="icon"
          onClick={() => handleNavigate(SidebarViewMode.DATABASE)}
          tooltip={{ content: "Database Client", side: "right" }}
          className={cn(
            "h-8 w-8",
            currentViewMode === SidebarViewMode.DATABASE &&
              "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
        >
          <Database className="h-4 w-4" />
        </Button>

        <Button
          variant="ghost"
          size="icon"
          onClick={() => handleNavigate(SidebarViewMode.SETTINGS)}
          tooltip={{ content: "Settings", side: "right" }}
          className={cn(
            "h-8 w-8",
            currentViewMode === SidebarViewMode.SETTINGS &&
              "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
        >
          <Settings className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
};

export default Sidebar;
