import { Pencil, Workflow } from "lucide-react";
import type React from "react";
import { useNavigate } from "react-router-dom";
import PageHeader from "@/components/PageHeader";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";

type WorkflowPageHeaderProps = {
  path: string;
  runId?: string;
};

const WorkflowPageHeader: React.FC<WorkflowPageHeaderProps> = ({ path, runId }) => {
  const relativePath = path;
  const pathb64 = btoa(path);
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();

  return (
    <PageHeader className='items-center border-border border-b-1'>
      <div className='flex w-full items-center justify-between'>
        <div></div>
        <div className='flex items-center justify-center gap-0.5'>
          <Workflow className='h-4 w-4' />
          <span className='truncate text-sm'>
            {relativePath}
            {runId ? `/runs/${runId}` : ""}
          </span>
        </div>
        <div className='flex items-center gap-2'>
          <Button
            size='sm'
            variant='ghost'
            onClick={() => {
              const fileUri = ROUTES.PROJECT(project.id).IDE.FILES.FILE(pathb64);
              navigate(fileUri);
            }}
          >
            <Pencil className='h-4 w-4' />
            <span>Edit</span>
          </Button>
        </div>
      </div>
    </PageHeader>
  );
};

export default WorkflowPageHeader;
