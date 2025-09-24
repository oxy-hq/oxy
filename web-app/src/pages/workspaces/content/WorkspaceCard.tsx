import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Badge } from "@/components/ui/shadcn/badge";
import { Building2, Users } from "lucide-react";
import { Workspace } from "@/types/workspace";

interface WorkspaceCardProps {
  workspace: Workspace;
  onWorkspaceClick: (workspace: Workspace) => void;
}

const WorkspaceCard = ({ workspace, onWorkspaceClick }: WorkspaceCardProps) => {
  return (
    <Card
      className="hover:shadow-md transition-shadow cursor-pointer group"
      onClick={() => onWorkspaceClick(workspace)}
    >
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-2">
            <Building2 className="h-5 w-5 text-primary" />
            <CardTitle className="text-lg group-hover:text-primary transition-colors">
              {workspace.name}
            </CardTitle>
          </div>
          <Badge variant={workspace.role === "owner" ? "default" : "secondary"}>
            {workspace.role}
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-2 text-sm text-muted-foreground">
          <div className="flex items-center gap-2">
            <Users className="h-4 w-4" />
            <span>
              Member since {new Date(workspace.created_at).toLocaleDateString()}
            </span>
          </div>
          {workspace.project && (
            <div className="flex items-center gap-2 mt-2 pt-2 border-t border-border">
              <Badge variant="outline">Project</Badge>
              <span className="font-medium">{workspace.project.name}</span>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
};

export default WorkspaceCard;
