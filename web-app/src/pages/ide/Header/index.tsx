import { Github, Home } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Card } from "@/components/ui/shadcn/card";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { VersionBadge } from "@/components/VersionBadge";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";
import useCurrentProject from "@/stores/useCurrentProject";
import { BranchInfo } from "./BranchInfo";
import { BranchSettings } from "./BranchSettings";

export const Header = () => {
  const { authConfig } = useAuth();
  const { project } = useCurrentProject();
  const { setOpen } = useSidebar();
  const navigate = useNavigate();
  const [isBranchSettingOpen, setIsBranchSettingOpen] = useState(false);

  const renderContent = () => {
    if (!authConfig.cloud) {
      return <div className='text-muted-foreground text-sm'>Local mode</div>;
    }
    return (
      <>
        {project?.project_repo_id ? (
          <BranchInfo />
        ) : (
          <div className='text-muted-foreground text-sm'>No repository connected</div>
        )}
        <Button
          variant='outline'
          size='sm'
          onClick={() => setIsBranchSettingOpen(true)}
          className='flex items-center gap-2 transition-colors hover:bg-accent/50'
        >
          <Github className='h-4 w-4' />
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
    <Card className='flex gap-2 rounded-none border-b bg-sidebar-background p-1 shadow-none'>
      <Button
        variant='ghost'
        size='sm'
        onClick={handleHomeClick}
        tooltip={{ content: "Back to Home", side: "right" }}
      >
        <Home className='h-4 w-4' />
      </Button>
      <div className='flex flex-1 items-center justify-between'>{renderContent()}</div>
      <VersionBadge />

      <BranchSettings isOpen={isBranchSettingOpen} onClose={() => setIsBranchSettingOpen(false)} />
    </Card>
  );
};

export default Header;
