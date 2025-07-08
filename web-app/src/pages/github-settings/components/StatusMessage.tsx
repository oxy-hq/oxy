import { AlertCircle } from "lucide-react";

interface StatusMessageProps {
  isTokenConfigured: boolean;
  selectedRepoId?: number;
}

export const StatusMessage = ({
  isTokenConfigured,
  selectedRepoId,
}: StatusMessageProps) => {
  if (!isTokenConfigured) {
    return (
      <div className="flex items-start gap-3 p-4 border rounded-lg bg-blue-50 dark:bg-blue-950/20">
        <AlertCircle className="h-5 w-5 text-blue-600 dark:text-blue-400 mt-0.5 flex-shrink-0" />
        <div className="text-sm">
          <p className="font-medium text-blue-900 dark:text-blue-100 mb-1">
            Configure GitHub Integration
          </p>
          <p className="text-blue-800 dark:text-blue-200">
            Configure your GitHub token to enable repository management and
            automatic synchronization. You'll need a Personal Access Token with{" "}
            <code className="px-1.5 py-0.5 bg-blue-100 dark:bg-blue-900 rounded text-xs">
              repo
            </code>
            ,{" "}
            <code className="px-1.5 py-0.5 bg-blue-100 dark:bg-blue-900 rounded text-xs">
              user:email
            </code>
            ,
            <code className="px-1.5 py-0.5 bg-blue-100 dark:bg-blue-900 rounded text-xs">
              read:user
            </code>
            , and{" "}
            <code className="px-1.5 py-0.5 bg-blue-100 dark:bg-blue-900 rounded text-xs">
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
      <div className="flex items-start gap-3 p-4 border rounded-lg bg-amber-50 dark:bg-amber-950/20">
        <AlertCircle className="h-5 w-5 text-amber-600 dark:text-amber-400 mt-0.5 flex-shrink-0" />
        <div className="text-sm">
          <p className="font-medium text-amber-900 dark:text-amber-100 mb-1">
            Select Repository
          </p>
          <p className="text-amber-800 dark:text-amber-200">
            Your GitHub token is configured. You can now select a repository
            from your onboarding or project selection flow to start working with
            your GitHub projects.
          </p>
        </div>
      </div>
    );
  }

  return null;
};
