import { useEffect, useState, useRef } from "react";
import { useCreateGitNamespace } from "@/hooks/api/github";
import { getInstallationInfoFromUrl } from "@/utils/githubAppInstall";
import { useNavigate } from "react-router-dom";
import { Loader2 } from "lucide-react";

/**
 * GitHub callback page that handles redirects from GitHub App installations
 * This page extracts installation_id and state from URL parameters and creates a git namespace
 */
export default function GitHubCallback() {
  const [error, setError] = useState<string | null>(null);
  const hasRunRef = useRef(false);
  const navigate = useNavigate();
  const createGitNamespace = useCreateGitNamespace();

  useEffect(() => {
    // Prevent running multiple times
    if (hasRunRef.current) return;

    async function handleCallback() {
      try {
        hasRunRef.current = true;

        // Extract installation info from URL
        const { installationId, state, code } = getInstallationInfoFromUrl();

        if (!installationId || !state) {
          setError("Missing required parameters in callback URL");
          return;
        }

        // Create git namespace with installation_id and state
        await createGitNamespace.mutateAsync(
          { installation_id: installationId, state, code },
          {
            onError: (err: Error | unknown) => {
              const errorMsg =
                err instanceof Error ? err.message : "Unknown error";
              setError(`Failed to process GitHub installation: ${errorMsg}`);
            },
          },
        );
      } catch (err: Error | unknown) {
        const errorMsg = err instanceof Error ? err.message : "Unknown error";
        setError(`Unexpected error: ${errorMsg}`);
      }
    }

    handleCallback();
  }, [navigate, createGitNamespace]);

  return (
    <div className="flex min-h-screen min-w-screen items-center justify-center">
      <div className="text-center">
        {error ? (
          <div className="p-6 bg-destructive/10 border border-destructive rounded-md max-w-md">
            <h2 className="text-xl font-bold mb-2">Installation Error</h2>
            <p className="text-destructive mb-4">{error}</p>
            <button
              className="px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90"
              onClick={() => navigate("/workspaces/new")}
            >
              Return to Workspace Creation
            </button>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-4">
            <Loader2 className="h-12 w-12 animate-spin text-primary" />
            <h2 className="text-xl font-medium">
              Processing GitHub Installation...
            </h2>
            <p className="text-muted-foreground">
              Please wait while we configure your GitHub connection.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
