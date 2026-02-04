import { Building2, Users } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/shadcn/card";
import type { Workspace } from "@/types/workspace";

interface WorkspaceCardProps {
  workspace: Workspace;
  onWorkspaceClick: (workspace: Workspace) => void;
}

const WorkspaceCard = ({ workspace, onWorkspaceClick }: WorkspaceCardProps) => {
  return (
    <Card
      className='group cursor-pointer transition-shadow hover:shadow-md'
      onClick={() => onWorkspaceClick(workspace)}
    >
      <CardHeader className='pb-3'>
        <div className='flex items-start justify-between'>
          <div className='flex items-center gap-2'>
            <Building2 className='h-5 w-5 text-primary' />
            <CardTitle className='text-lg transition-colors group-hover:text-primary'>
              {workspace.name}
            </CardTitle>
          </div>
          <Badge variant={workspace.role === "owner" ? "default" : "secondary"}>
            {workspace.role}
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <div className='space-y-2 text-muted-foreground text-sm'>
          <div className='flex items-center gap-2'>
            <Users className='h-4 w-4' />
            <span>Member since {new Date(workspace.created_at).toLocaleDateString()}</span>
          </div>
          {workspace.project && (
            <div className='mt-2 flex items-center gap-2 border-border border-t pt-2'>
              <Badge variant='outline'>Project</Badge>
              <span className='font-medium'>{workspace.project.name}</span>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
};

export default WorkspaceCard;
