import { AlertCircle } from "lucide-react";

interface StatusMessageProps {
  isTokenConfigured: boolean;
  selectedRepoId?: number;
}

export const StatusMessage = ({ isTokenConfigured, selectedRepoId }: StatusMessageProps) => {
  if (!isTokenConfigured) {
    return (
      <div className='flex items-start gap-3 rounded-lg border bg-info/10 p-4'>
        <AlertCircle className='mt-0.5 h-5 w-5 flex-shrink-0 text-info' />
        <div className='text-sm'>
          <p className='mb-1 font-medium text-info'>Configure GitHub Integration</p>
          <p className='text-info'>
            Configure your GitHub token to enable repository management and automatic
            synchronization. You'll need a Personal Access Token with{" "}
            <code className='rounded bg-info/10 px-1.5 py-0.5 text-xs'>repo</code>,{" "}
            <code className='rounded bg-info/10 px-1.5 py-0.5 text-xs'>user:email</code>,
            <code className='rounded bg-info/10 px-1.5 py-0.5 text-xs'>read:user</code>, and{" "}
            <code className='rounded bg-info/10 px-1.5 py-0.5 text-xs'>admin:repo_hook</code>{" "}
            permissions.
          </p>
        </div>
      </div>
    );
  }

  if (isTokenConfigured && !selectedRepoId) {
    return (
      <div className='flex items-start gap-3 rounded-lg border bg-warning/10 p-4'>
        <AlertCircle className='mt-0.5 h-5 w-5 flex-shrink-0 text-warning' />
        <div className='text-sm'>
          <p className='mb-1 font-medium text-warning'>Select Repository</p>
          <p className='text-warning'>
            Your GitHub token is configured. You can now select a repository from your onboarding or
            project selection flow to start working with your GitHub projects.
          </p>
        </div>
      </div>
    );
  }

  return null;
};
