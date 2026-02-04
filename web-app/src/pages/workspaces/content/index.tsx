import type { Workspace } from "@/types/workspace";
import EmptyState from "./EmptyState";
import LoadingState from "./LoadingState";
import WorkspaceCard from "./WorkspaceCard";

interface Props {
  workspaces?: Workspace[];
  filteredWorkspaces: Workspace[];
  searchQuery: string;
  isLoading: boolean;
  error: Error | null;
  onWorkspaceClick: (workspace: Workspace) => void;
  onClearSearch: () => void;
  onRetry: () => void;
}

const Content = ({
  workspaces,
  filteredWorkspaces,
  searchQuery,
  isLoading,
  error,
  onWorkspaceClick,
  onClearSearch,
  onRetry
}: Props) => {
  if (isLoading) {
    return <LoadingState />;
  }

  if (error) {
    return <LoadingState error={error} onRetry={onRetry} />;
  }

  if (!workspaces || workspaces.length === 0) {
    return <EmptyState type='no-workspaces' />;
  }

  if (filteredWorkspaces.length === 0 && searchQuery.trim()) {
    return <EmptyState type='no-search-results' onClearSearch={onClearSearch} />;
  }

  return (
    <div className='grid gap-4 md:grid-cols-2 lg:grid-cols-3'>
      {filteredWorkspaces.map((workspace) => (
        <WorkspaceCard
          key={workspace.id}
          workspace={workspace}
          onWorkspaceClick={onWorkspaceClick}
        />
      ))}
    </div>
  );
};

export default Content;
