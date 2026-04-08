import { CreateRepository } from "@/components/CreateRepository";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import PageWrapper from "../components/PageWrapper";
import RepositoryInfoSection from "./RepositoryInfoSection";

export default function GithubSettings() {
  const { workspace: project } = useCurrentWorkspace();
  return (
    <PageWrapper title='GitHub'>
      {project?.project_repo_id ? <RepositoryInfoSection /> : <CreateRepository />}
    </PageWrapper>
  );
}
