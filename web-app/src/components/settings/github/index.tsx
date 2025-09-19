import { Separator } from "@/components/ui/shadcn/separator";
import PageWrapper from "../components/PageWrapper";
import { GitHubTokenSection } from "./GitHubTokenSection";
import RepositoryInfoSection from "./RepositoryInfoSection";

export default function GithubSettings() {
  return (
    <PageWrapper title="GitHub">
      <RepositoryInfoSection />
      <Separator className="my-6" />
      <GitHubTokenSection />
    </PageWrapper>
  );
}
