import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Pencil, Workflow } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import { useNavigate } from "react-router-dom";

type WorkflowPageHeaderProps = {
  path: string;
};

const WorkflowPageHeader: React.FC<WorkflowPageHeaderProps> = ({ path }) => {
  const relativePath = path;
  const pathb64 = btoa(path);
  const navigate = useNavigate();
  return (
    <PageHeader className="border-b-1 border-border items-center">
      <div className="flex justify-between items-center w-full">
        <div></div>
        <div className="flex items-center justify-center gap-0.5">
          <Workflow className="h-4 w-4" />
          <span className="text-sm truncate">{relativePath}</span>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              navigate(`/ide/${pathb64}`);
            }}
          >
            <Pencil className="w-4 h-4" />
            <span>Edit</span>
          </Button>
        </div>
      </div>
    </PageHeader>
  );
};

export default WorkflowPageHeader;
