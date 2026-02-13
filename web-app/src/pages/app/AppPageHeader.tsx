import { LayoutDashboard, LoaderCircle } from "lucide-react";
import type React from "react";
import PageHeader from "@/components/PageHeader";
import { Button } from "@/components/ui/shadcn/button";

type AppPageHeaderProps = {
  path: string;
  onRun: () => void;
  isRunning: boolean;
};

const AppPageHeader: React.FC<AppPageHeaderProps> = ({ path, onRun, isRunning }) => {
  const relativePath = path;
  return (
    <PageHeader className='border-border border-b-1'>
      <div className='flex w-full items-center justify-between'>
        <div />
        <div className='flex items-center justify-center gap-0.5'>
          <LayoutDashboard width={16} height={16} />
          <span className='truncate text-sm'>{relativePath}</span>
        </div>
        <div className='flex items-center gap-2'>
          <Button size='sm' onClick={onRun} disabled={isRunning} variant='default' content='icon'>
            {isRunning ? <LoaderCircle className='animate-spin' /> : "Refresh"}
          </Button>
        </div>
      </div>
    </PageHeader>
  );
};

export default AppPageHeader;
