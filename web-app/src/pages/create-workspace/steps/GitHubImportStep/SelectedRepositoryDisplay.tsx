import { Github } from "lucide-react";
import { GitHubRepository } from "@/types/github";

interface SelectedRepositoryDisplayProps {
  repository: GitHubRepository;
}

export default function SelectedRepositoryDisplay({
  repository,
}: SelectedRepositoryDisplayProps) {
  return (
    <div className="border rounded-md p-3 space-y-2">
      <div className="flex items-center gap-2">
        <Github className="h-4 w-4 text-muted-foreground" />
        <h4 className="font-medium">{repository.full_name}</h4>
      </div>
      {repository.description && (
        <p className="text-sm text-muted-foreground">
          {repository.description}
        </p>
      )}
      <div className="text-xs text-muted-foreground">
        Default branch:{" "}
        <span className="font-mono">{repository.default_branch}</span>
      </div>
      <div className="text-xs text-muted-foreground">
        Last updated: {new Date(repository.updated_at).toLocaleDateString()}
      </div>
    </div>
  );
}
