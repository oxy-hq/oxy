import { Separator } from "@/components/ui/shadcn/separator";
import {
  useGithubSettings,
  useRevisionInfo,
} from "@/hooks/api/useGithubSettings";
import { GitHubTokenSection } from "./GitHubTokenSection";
import { RepositorySelector } from "./RepositorySelector";
import { RepositoryInfoSection } from "./RepositoryInfoSection";
import { StatusMessage } from "./StatusMessage";
import PageWrapper from "../components/PageWrapper";

export default function GithubSettings() {
  const { data: settings, isLoading: loading } = useGithubSettings();
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo(
    !!settings?.token_configured && !!settings?.selected_repo_id,
    false,
    false,
  );

  return (
    <PageWrapper title="GitHub" loading={loading}>
      <GitHubTokenSection isTokenConfigured={!!settings?.token_configured} />

      <Separator className="my-6" />

      <RepositorySelector
        isTokenConfigured={!!settings?.token_configured}
        selectedRepoId={settings?.selected_repo_id}
      />

      <Separator className="my-6" />

      {settings?.selected_repo_id && (
        <RepositoryInfoSection
          repositoryName={settings?.repository_name}
          revisionInfo={revisionInfo}
          revisionLoading={revisionLoading}
        />
      )}

      <StatusMessage
        isTokenConfigured={!!settings?.token_configured}
        selectedRepoId={settings?.selected_repo_id}
      />
    </PageWrapper>
  );
}
