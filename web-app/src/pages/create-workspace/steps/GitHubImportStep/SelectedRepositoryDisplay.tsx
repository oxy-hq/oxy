import { Github } from "lucide-react";
import type { GitHubRepository } from "@/types/github";

interface SelectedRepositoryDisplayProps {
  repository: GitHubRepository;
}

export default function SelectedRepositoryDisplay({ repository }: SelectedRepositoryDisplayProps) {
  return (
    <div className='space-y-2 rounded-md border p-3'>
      <div className='flex items-center gap-2'>
        <Github className='h-4 w-4 text-muted-foreground' />
        <h4 className='font-medium'>{repository.full_name}</h4>
      </div>
      {repository.description && (
        <p className='text-muted-foreground text-sm'>{repository.description}</p>
      )}
      <div className='text-muted-foreground text-xs'>
        Default branch: <span className='font-mono'>{repository.default_branch}</span>
      </div>
      <div className='text-muted-foreground text-xs'>
        Last updated: {new Date(repository.updated_at).toLocaleDateString()}
      </div>
    </div>
  );
}
