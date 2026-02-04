import { AlertCircle } from "lucide-react";

interface StatusMessageProps {
  isTokenConfigured: boolean;
  selectedRepoId?: number;
}

export const StatusMessage = ({ isTokenConfigured, selectedRepoId }: StatusMessageProps) => {
  if (!isTokenConfigured) {
    return (
      <div className='flex items-start gap-3 rounded-lg border bg-blue-50 p-4 dark:bg-blue-950/20'>
        <AlertCircle className='mt-0.5 h-5 w-5 flex-shrink-0 text-blue-600 dark:text-blue-400' />
        <div className='text-sm'>
          <p className='mb-1 font-medium text-blue-900 dark:text-blue-100'>
            Configure GitHub Integration
          </p>
          <p className='text-blue-800 dark:text-blue-200'>
            Configure your GitHub token to enable repository management and automatic
            synchronization. You'll need a Personal Access Token with{" "}
            <code className='rounded bg-blue-100 px-1.5 py-0.5 text-xs dark:bg-blue-900'>repo</code>
            ,{" "}
            <code className='rounded bg-blue-100 px-1.5 py-0.5 text-xs dark:bg-blue-900'>
              user:email
            </code>
            ,
            <code className='rounded bg-blue-100 px-1.5 py-0.5 text-xs dark:bg-blue-900'>
              read:user
            </code>
            , and{" "}
            <code className='rounded bg-blue-100 px-1.5 py-0.5 text-xs dark:bg-blue-900'>
              admin:repo_hook
            </code>{" "}
            permissions.
          </p>
        </div>
      </div>
    );
  }

  if (isTokenConfigured && !selectedRepoId) {
    return (
      <div className='flex items-start gap-3 rounded-lg border bg-amber-50 p-4 dark:bg-amber-950/20'>
        <AlertCircle className='mt-0.5 h-5 w-5 flex-shrink-0 text-amber-600 dark:text-amber-400' />
        <div className='text-sm'>
          <p className='mb-1 font-medium text-amber-900 dark:text-amber-100'>Select Repository</p>
          <p className='text-amber-800 dark:text-amber-200'>
            Your GitHub token is configured. You can now select a repository from your onboarding or
            project selection flow to start working with your GitHub projects.
          </p>
        </div>
      </div>
    );
  }

  return null;
};
