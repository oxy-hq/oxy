import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Loader2, ExternalLink, AlertCircle } from "lucide-react";
import { toast } from "sonner";
import useUpdateGitHubToken from "@/hooks/api/projects/useUpdateGitHubToken";

export const GitHubTokenSection = () => {
  const updateGitHubTokenMutation = useUpdateGitHubToken();
  const [token, setToken] = useState("");
  const [showTokenInput, setShowTokenInput] = useState(false);

  const updateGitHubToken = async () => {
    if (!token.trim()) {
      toast.error("Please enter a GitHub token");
      return;
    }

    const result = await updateGitHubTokenMutation.mutateAsync(token);
    if (result.success) {
      toast.success("GitHub token updated successfully");
      setToken("");
      setShowTokenInput(false);
    } else {
      toast.error(result.message || "Failed to update GitHub token");
    }
  };

  return (
    <>
      <div className="flex items-start justify-between">
        <div className="space-y-1">
          <Label className="text-sm">GitHub Token</Label>
          <p className="text-sm text-muted-foreground">
            Personal Access Token for GitHub API access
          </p>
        </div>

        {!showTokenInput && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowTokenInput(!showTokenInput)}
          >
            Update
          </Button>
        )}
      </div>

      {showTokenInput && (
        <div className="space-y-3 p-4 border rounded-lg bg-muted/50 mt-4">
          <div className="space-y-2">
            <Label htmlFor="github-token">GitHub Personal Access Token</Label>
            <Input
              id="github-token"
              type="password"
              placeholder="ghp_..."
              value={token}
              onChange={(e) => setToken(e.target.value)}
            />
            <p className="text-xs text-muted-foreground">
              Need a token?{" "}
              <a
                href="https://github.com/settings/tokens/new"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline inline-flex items-center gap-1"
              >
                Create one on GitHub
                <ExternalLink className="h-3 w-3" />
              </a>
            </p>
          </div>
          <div className="flex items-start gap-3 p-4 border rounded-lg bg-blue-50 dark:bg-blue-950/20">
            <AlertCircle className="h-5 w-5 text-blue-600 dark:text-blue-400 mt-0.5 flex-shrink-0" />
            <div className="text-sm">
              <p className="font-medium text-blue-900 dark:text-blue-100 mb-1">
                Configure GitHub Integration
              </p>
              <p className="text-blue-800 dark:text-blue-200">
                Configure your GitHub token to enable repository management and
                automatic synchronization. You'll need a Personal Access Token
                with{" "}
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
          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={updateGitHubToken}
              disabled={updateGitHubTokenMutation.isPending}
            >
              {updateGitHubTokenMutation.isPending && (
                <Loader2 className="animate-spin" />
              )}
              Save Token
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setShowTokenInput(false)}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
    </>
  );
};
