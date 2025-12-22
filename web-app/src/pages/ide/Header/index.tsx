import { useState } from "react";
import { Card } from "@/components/ui/shadcn/card";
import { Button } from "@/components/ui/shadcn/button";
import { Github, Home } from "lucide-react";
import { BranchInfo } from "./BranchInfo";
import { BranchSettings } from "./BranchSettings";
import useCurrentProject from "@/stores/useCurrentProject";
import { useAuth } from "@/contexts/AuthContext";
import { useNavigate } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";
import useSidebar from "@/components/ui/shadcn/sidebar-context";

export const Header = () => {
  const { authConfig } = useAuth();
  const { project } = useCurrentProject();
  const { setOpen } = useSidebar();
  const navigate = useNavigate();
  const [isBranchSettingOpen, setIsBranchSettingOpen] = useState(false);

  const renderContent = () => {
    if (!authConfig.cloud) {
      return <div className="text-sm text-muted-foreground">Local mode</div>;
    }
    return (
      <>
        {project?.project_repo_id ? (
          <BranchInfo />
        ) : (
          <div className="text-sm text-muted-foreground">
            No repository connected
          </div>
        )}
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsBranchSettingOpen(true)}
          className="flex items-center gap-2 hover:bg-accent/50 transition-colors"
        >
          <Github className="w-4 h-4" />
        </Button>
      </>
    );
  };

  const homeRoute = project?.id ? ROUTES.PROJECT(project.id).HOME : ROUTES.ROOT;

  const handleHomeClick = () => {
    setOpen(true);
    navigate(homeRoute);
  };

  return (
    <Card className="flex gap-2 p-1 border-b bg-sidebar-background shadow-none rounded-none ">
      <Button
        variant="ghost"
        size="sm"
        onClick={handleHomeClick}
        tooltip="Back to Home"
      >
        <Home className="w-4 h-4" />
      </Button>
      <div className="flex items-center justify-between flex-1">
        {renderContent()}
      </div>

      <BranchSettings
        isOpen={isBranchSettingOpen}
        onClose={() => setIsBranchSettingOpen(false)}
      />
    </Card>
  );
};

export default Header;
