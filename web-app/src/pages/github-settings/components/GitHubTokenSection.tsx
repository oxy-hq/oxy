import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Badge } from "@/components/ui/shadcn/badge";
import { Loader2, ExternalLink, CheckCircle, AlertCircle } from "lucide-react";
import { toast } from "sonner";
import { useUpdateGitHubToken } from "@/hooks/api/useGithubSettings";

interface GitHubTokenSectionProps {
  isTokenConfigured: boolean;
}

export const GitHubTokenSection = ({
  isTokenConfigured,
}: GitHubTokenSectionProps) => {
  const updateGitHubTokenMutation = useUpdateGitHubToken();
  const [token, setToken] = useState("");
  const [showTokenInput, setShowTokenInput] = useState(false);

  const updateGitHubToken = async () => {
    if (!token.trim()) {
      toast.error("Please enter a GitHub token");
      return;
    }

    await updateGitHubTokenMutation.mutateAsync(token);
    setToken("");
    setShowTokenInput(false);
  };

  return (
    <>
      {/* Token Status */}
      <div className="flex items-center justify-between">
        <div className="space-y-1">
          <Label className="text-sm font-medium">GitHub Token</Label>
          <p className="text-sm text-muted-foreground">
            Personal Access Token for GitHub API access
          </p>
        </div>
        <div className="flex items-center gap-2">
          {isTokenConfigured ? (
            <Badge variant="secondary" className="flex items-center gap-1">
              <CheckCircle className="h-3 w-3" />
              Configured
            </Badge>
          ) : (
            <Badge variant="destructive" className="flex items-center gap-1">
              <AlertCircle className="h-3 w-3" />
              Not Configured
            </Badge>
          )}
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowTokenInput(!showTokenInput)}
          >
            {isTokenConfigured ? "Update" : "Configure"}
          </Button>
        </div>
      </div>

      {showTokenInput && (
        <div className="space-y-3 p-4 border rounded-lg bg-muted/50">
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
          <div className="flex gap-2">
            <Button
              onClick={updateGitHubToken}
              disabled={updateGitHubTokenMutation.isPending}
            >
              {updateGitHubTokenMutation.isPending && (
                <Loader2 className="animate-spin h-4 w-4 mr-2" />
              )}
              Save Token
            </Button>
            <Button variant="outline" onClick={() => setShowTokenInput(false)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </>
  );
};
