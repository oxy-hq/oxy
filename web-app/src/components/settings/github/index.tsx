import PageWrapper from "../components/PageWrapper";
import RepositoryInfoSection from "./RepositoryInfoSection";
import useCurrentProject from "@/stores/useCurrentProject";
import { CreateRepository } from "@/components/CreateRepository";

export default function GithubSettings() {
  const { project } = useCurrentProject();
  return (
    <PageWrapper title="GitHub">
      {project?.project_repo_id ? (
        <RepositoryInfoSection />
      ) : (
        <CreateRepository />
      )}
    </PageWrapper>
  );
}
