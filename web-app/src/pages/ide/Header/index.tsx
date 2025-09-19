import { useState } from "react";
import { Card } from "@/components/ui/shadcn/card";
import { Button } from "@/components/ui/shadcn/button";
import { Github } from "lucide-react";
import { BranchInfo } from "./BranchInfo";
import { BranchSettings } from "./BranchSettings";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";

export const Header = () => {
  const [isBranchSettingOpen, setIsBranchSettingOpen] = useState(false);
  const { open } = useSidebar();
  return (
    <Card className="flex gap-2 p-2 border-b bg-sidebar-background shadow-none rounded-none ">
      {!open && <SidebarTrigger />}
      <div className="flex items-center justify-between flex-1">
        <BranchInfo />
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsBranchSettingOpen(true)}
          className="flex items-center gap-2 hover:bg-accent/50 transition-colors"
        >
          <Github className="w-4 h-4" />
        </Button>
      </div>

      <BranchSettings
        isOpen={isBranchSettingOpen}
        onClose={() => setIsBranchSettingOpen(false)}
      />
    </Card>
  );
};

export default Header;
