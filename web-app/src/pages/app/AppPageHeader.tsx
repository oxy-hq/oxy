import { LayoutDashboard } from "lucide-react";
import type React from "react";
import PageHeader from "@/components/PageHeader";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useApps from "@/hooks/api/apps/useApps";

type AppPageHeaderProps = {
  path: string;
  onRun: () => void;
  isRunning: boolean;
};

const AppPageHeader: React.FC<AppPageHeaderProps> = ({ path, onRun, isRunning }) => {
  const { data: apps } = useApps();
  const isDraft = apps?.some((a) => a.path === path && !a.published) ?? false;
  return (
    <PageHeader className='border-border border-b-1'>
      <div className='flex w-full items-center justify-between'>
        <div />
        <div className='flex items-center justify-center gap-1.5'>
          <LayoutDashboard width={16} height={16} />
          <span className='truncate text-sm'>{path}</span>
          {isDraft && (
            <Badge variant='outline' className='font-normal text-[10px] text-muted-foreground'>
              Draft
            </Badge>
          )}
        </div>
        <div className='flex items-center gap-2'>
          <Button size='sm' onClick={onRun} disabled={isRunning} variant='default' content='icon'>
            {isRunning ? <Spinner /> : "Refresh"}
          </Button>
        </div>
      </div>
    </PageHeader>
  );
};

export default AppPageHeader;
