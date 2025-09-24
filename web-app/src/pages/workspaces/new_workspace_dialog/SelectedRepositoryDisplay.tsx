import React from "react";
import { Label } from "@/components/ui/shadcn/label";
import { Github } from "lucide-react";
import { GitHubRepository } from "@/types/github";

interface SelectedRepositoryDisplayProps {
  repository: GitHubRepository;
}

export const SelectedRepositoryDisplay: React.FC<
  SelectedRepositoryDisplayProps
> = ({ repository }) => {
  return (
    <div className="space-y-2">
      <Label>Selected Repository</Label>
      <div className="p-3 border rounded-lg bg-muted/50">
        <div className="flex items-center gap-2 mb-2">
          <Github className="h-4 w-4 text-muted-foreground" />
          <span className="font-medium">{repository.full_name}</span>
        </div>
        {repository.description && (
          <p className="text-sm text-muted-foreground mb-2">
            {repository.description}
          </p>
        )}
        <div className="flex items-center gap-4 text-xs text-muted-foreground">
          <span>Default branch: {repository.default_branch}</span>
          <span>Project name: {repository.name}</span>
        </div>
      </div>
    </div>
  );
};
