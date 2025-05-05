import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { LayoutDashboard, LoaderCircle } from "lucide-react";
import PageHeader from "@/components/PageHeader";

type AppPageHeaderProps = {
  path: string;
  onRun: () => void;
  isRunning: boolean;
};

const AppPageHeader: React.FC<AppPageHeaderProps> = ({
  path,
  onRun,
  isRunning,
}) => {
  const relativePath = path;
  return (
    <PageHeader className="border-b-1 border-border">
      <div className="flex justify-between items-center w-full">
        <div />
        <div className="flex items-center justify-center gap-0.5">
          <LayoutDashboard width={16} height={16} />
          <span className="text-sm truncate">{relativePath}</span>
        </div>
        <Button
          size="sm"
          onClick={onRun}
          disabled={isRunning}
          variant="default"
          content="icon"
        >
          {isRunning ? <LoaderCircle className="animate-spin" /> : "Refresh"}
        </Button>
      </div>
    </PageHeader>
  );
};

export default AppPageHeader;
