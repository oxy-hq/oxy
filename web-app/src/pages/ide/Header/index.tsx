import { Home } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Card } from "@/components/ui/shadcn/card";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import useRevisionInfo from "@/hooks/api/workspaces/useRevisionInfo";
import { FEATURES } from "@/libs/features";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useIdeBranch from "@/stores/useIdeBranch";
import useSelectedRepo from "@/stores/useSelectedRepo";
import { GitActions } from "./components/GitActions";
import { GithubIcon } from "./components/GithubIcon";
import { IDEProjectSwitcher } from "./components/IDEProjectSwitcher";
import { LinkedRepoActions } from "./components/LinkedRepoActions";
import { RepoSwitcher } from "./components/RepoSwitcher";
import { IdeGitProvider } from "./context/IdeGitContext";
import { useGithubUrls } from "./hooks/useGithubUrls";

export const OPEN_BRANCH_SETTINGS = "ide:open-branch-settings";

// Stable reference so memoised consumers don't invalidate every render
// before the workspace loads.
const EMPTY_PROTECTED_BRANCHES: string[] = [];

export const Header = () => {
  const { workspace: project } = useCurrentWorkspace();
  const { selectedRepo } = useSelectedRepo();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const isLinkedRepo = selectedRepo !== "primary";
  const { setOpen } = useSidebar();
  const navigate = useNavigate();

  // `useCurrentWorkspaceBranch` throws without a workspace; Header renders
  // during initial load too, so resolve the branch manually.
  const { getCurrentBranch } = useIdeBranch();
  const activeBranch = project?.active_branch?.name ?? "";
  const ideBranch = project ? (getCurrentBranch(project.id) ?? activeBranch) : activeBranch;
  const defaultBranch = project?.default_branch ?? "main";
  const isOnMain = ideBranch === defaultBranch;

  const caps = project?.capabilities;
  const canCommit = !!caps?.can_commit;
  const canDiff = !!caps?.can_diff;
  const protectedBranches = project?.protected_branches ?? EMPTY_PROTECTED_BRANCHES;

  // Header sits above <IdeGitProvider> so we read revisionInfo directly;
  // React Query dedupes by key so the provider's internal call shares it.
  const { data: revisionInfo } = useRevisionInfo(canDiff && !!project?.id);
  const { repoUrl: githubRepoUrl } = useGithubUrls({
    remoteUrl: revisionInfo?.remote_url,
    branch: ideBranch,
    defaultBranch,
    isOnMain
  });

  const handleHomeClick = () => {
    setOpen(true);
    navigate(project?.id ? ROUTES.ORG(orgSlug).WORKSPACE(project.id).HOME : ROUTES.ROOT);
  };

  return (
    <Card className='flex gap-2 rounded-none border-y-0 border-t-0 border-b bg-sidebar-background p-1 shadow-none'>
      <SidebarTrigger className='h-8 w-8 md:hidden' />
      <Button
        variant='ghost'
        size='sm'
        onClick={handleHomeClick}
        tooltip={{ content: "Back to Home", side: "right" }}
        className='h-8 w-8'
      >
        <Home className='h-4 w-4' />
      </Button>
      <IDEProjectSwitcher />
      {githubRepoUrl && !isLinkedRepo && (
        <Button
          size='sm'
          onClick={(e) => {
            e.preventDefault();
            window.open(githubRepoUrl, "_blank", "noopener");
          }}
          title='Open on GitHub'
          variant='ghost'
        >
          <GithubIcon className='h-3.5 w-3.5' />
        </Button>
      )}
      <div className='flex flex-1 items-center justify-end gap-2'>
        {FEATURES.LINKED_REPOS && <RepoSwitcher isReadOnly={!canCommit && !project?.id} />}
        {FEATURES.LINKED_REPOS && isLinkedRepo ? (
          <LinkedRepoActions repoName={selectedRepo} />
        ) : (
          <IdeGitProvider
            workspaceId={project?.id}
            branch={ideBranch}
            defaultBranch={defaultBranch}
            isOnMain={isOnMain}
            protectedBranches={protectedBranches}
            capabilities={project?.capabilities}
          >
            <GitActions />
          </IdeGitProvider>
        )}
      </div>
    </Card>
  );
};

export default Header;
