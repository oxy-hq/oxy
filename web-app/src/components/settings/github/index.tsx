import { CreateRepository } from "@/components/CreateRepository";
import useCurrentProject from "@/stores/useCurrentProject";
import PageWrapper from "../components/PageWrapper";
import RepositoryInfoSection from "./RepositoryInfoSection";

export default function GithubSettings() {
  const { project } = useCurrentProject();
  return (
    <PageWrapper title='GitHub'>
      {project?.project_repo_id ? <RepositoryInfoSection /> : <CreateRepository />}
    </PageWrapper>
  );
}
