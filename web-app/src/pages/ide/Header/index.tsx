import { useState } from "react";
import { Card } from "@/components/ui/shadcn/card";
import { Button } from "@/components/ui/shadcn/button";
import { Github } from "lucide-react";
import { BranchInfo } from "./BranchInfo";
import { BranchSettings } from "./BranchSettings";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import useCurrentProject from "@/stores/useCurrentProject";
import { useAuth } from "@/contexts/AuthContext";

export const Header = () => {
  const { authConfig } = useAuth();
  const { project } = useCurrentProject();
  const [isBranchSettingOpen, setIsBranchSettingOpen] = useState(false);
  const { open } = useSidebar();

  const renderContent = () => {
    if (authConfig.local) {
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

  return (
    <Card className="flex gap-2 p-2 border-b bg-sidebar-background shadow-none rounded-none ">
      {!open && <SidebarTrigger />}
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
