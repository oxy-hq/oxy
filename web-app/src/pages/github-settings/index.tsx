import { Card, CardContent } from "@/components/ui/shadcn/card";
import { Separator } from "@/components/ui/shadcn/separator";
import { Loader2, Github } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import {
  useGithubSettings,
  useRevisionInfo,
} from "@/hooks/api/useGithubSettings";
import { GitHubTokenSection } from "./components/GitHubTokenSection";
import { RepositorySelector } from "./components/RepositorySelector";
import { RepositoryInfoSection } from "./components/RepositoryInfoSection";
import { StatusMessage } from "./components/StatusMessage";

export default function GithubSettingsPage() {
  const { data: settings, isLoading: loading } = useGithubSettings();
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo(
    // Only fetch revision info if GitHub is configured and a repository is selected
    !!settings?.token_configured && !!settings?.selected_repo_id,
    false, // Don't refetch on window focus for revision info
    false,
  );

  // Show loading if either settings or required revision info is loading
  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="animate-spin h-6 w-6" />
      </div>
    );
  }

  return (
    <div className="container mx-auto py-6 space-y-6">
      <PageHeader>
        <div className="flex items-center gap-1">
          <Github />
          <h1 className="text-2xl font-bold">Github Settings</h1>
        </div>
      </PageHeader>

      {/* GitHub Integration Section */}
      <Card>
        <CardContent className="space-y-4 pt-4">
          <GitHubTokenSection
            isTokenConfigured={!!settings?.token_configured}
          />

          <Separator />

          <RepositorySelector
            isTokenConfigured={!!settings?.token_configured}
            selectedRepoId={settings?.selected_repo_id}
          />

          <Separator />

          {/* Repository Sync Status - Only show if repository is selected */}
          {settings?.selected_repo_id && (
            <>
              <RepositoryInfoSection
                repositoryName={settings?.repository_name}
                revisionInfo={revisionInfo}
                revisionLoading={revisionLoading}
              />
            </>
          )}

          {/* GitHub Integration Status Info */}
          <StatusMessage
            isTokenConfigured={!!settings?.token_configured}
            selectedRepoId={settings?.selected_repo_id}
          />
        </CardContent>
      </Card>
    </div>
  );
}
