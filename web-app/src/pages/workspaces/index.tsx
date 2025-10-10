import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import { Input } from "@/components/ui/shadcn/input";
import { Search, PlusCircle } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import ROUTES from "@/libs/utils/routes";
import Header from "./components/Header";
import Content from "./content";
import { Workspace } from "@/types/workspace";

export default function WorkspacesPage() {
  const navigate = useNavigate();
  const [searchQuery, setSearchQuery] = useState("");

  const { data: workspacesData, isLoading, error, refetch } = useWorkspaces();

  const filteredWorkspaces =
    workspacesData?.workspaces?.filter((workspace) =>
      workspace.name.toLowerCase().includes(searchQuery.toLowerCase()),
    ) || [];

  const handleWorkspaceClick = (workspace: Workspace) => {
    if (!workspace.project) return;
    navigate(ROUTES.PROJECT(workspace.project?.id).ROOT);
  };

  const handleClearSearch = () => setSearchQuery("");

  return (
    <div className="flex flex-col w-full overflow-auto customScrollbar">
      <Header />

      <div className="flex-1 p-6 max-w-6xl mx-auto max-w-[1200px] w-full">
        <div className="mb-6">
          <h1 className="text-2xl mb-4">Your Workspaces</h1>
          <div className="flex items-center justify-between">
            <div className="flex items-center">
              {workspacesData?.workspaces &&
                workspacesData.workspaces.length > 0 && (
                  <div className="relative">
                    <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                    <Input
                      placeholder="Search for a workspace"
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      className="pl-10 w-80"
                    />
                  </div>
                )}
            </div>
            <Button onClick={() => navigate(ROUTES.WORKSPACE.CREATE_WORKSPACE)}>
              <PlusCircle className="mr-2 h-4 w-4" />
              New Workspace
            </Button>
          </div>
        </div>

        <div className="space-y-6">
          <Content
            workspaces={workspacesData?.workspaces}
            filteredWorkspaces={filteredWorkspaces}
            searchQuery={searchQuery}
            isLoading={isLoading}
            error={error}
            onWorkspaceClick={handleWorkspaceClick}
            onClearSearch={handleClearSearch}
            onRetry={refetch}
          />
        </div>
      </div>
    </div>
  );
}
