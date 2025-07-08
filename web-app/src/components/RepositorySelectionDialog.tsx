import { useState, useEffect, useMemo } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { GitHubRepository } from "@/types/github";
import {
  useListRepositories,
  useSelectRepository,
} from "@/hooks/api/useGithubSettings";
import { Loader2, Search, Calendar, ExternalLink, Github } from "lucide-react";

interface RepositorySelectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function RepositorySelectionDialog({
  open,
  onOpenChange,
}: RepositorySelectionDialogProps) {
  const [searchQuery, setSearchQuery] = useState("");

  const { data: repositories = [], isLoading, refetch } = useListRepositories();
  const selectRepositoryMutation = useSelectRepository();

  useEffect(() => {
    if (open) {
      refetch();
    }
  }, [open, refetch]);

  const filteredRepos = useMemo(() => {
    if (searchQuery.trim()) {
      return repositories.filter(
        (repo) =>
          repo.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          repo.full_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          (repo.description &&
            repo.description.toLowerCase().includes(searchQuery.toLowerCase())),
      );
    }
    return repositories;
  }, [repositories, searchQuery]);

  const handleSelectRepository = async (repo: GitHubRepository) => {
    await selectRepositoryMutation.mutateAsync(repo.id);
    onOpenChange(false);
  };

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-hidden">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Github className="h-5 w-5" />
            Select Repository
          </DialogTitle>
          <DialogDescription>
            Choose a repository to work with. The repository will be cloned
            locally and synced automatically.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* Search */}
          <div className="relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 text-gray-400 h-4 w-4" />
            <Input
              placeholder="Search repositories..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-10"
            />
          </div>

          {/* Repository List */}
          <div className="overflow-y-auto max-h-96 space-y-3">
            {isLoading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin" />
                <span className="ml-2">Loading repositories...</span>
              </div>
            ) : (
              <>
                {filteredRepos.length === 0 ? (
                  <div className="text-center py-8 text-muted-foreground">
                    {searchQuery
                      ? "No repositories match your search."
                      : "No repositories found."}
                  </div>
                ) : (
                  filteredRepos.map((repo) => (
                    <Card
                      key={repo.id}
                      className="cursor-pointer hover:shadow-md transition-shadow"
                      onClick={() => handleSelectRepository(repo)}
                    >
                      <CardHeader className="pb-3">
                        <div className="flex items-start justify-between">
                          <div className="space-y-1">
                            <CardTitle className="text-lg flex items-center gap-2">
                              {repo.name}
                            </CardTitle>
                            <CardDescription className="text-sm text-muted-foreground">
                              {repo.full_name}
                            </CardDescription>
                          </div>
                          <div className="flex items-center gap-2">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={(e) => {
                                e.stopPropagation();
                                window.open(repo.html_url, "_blank");
                              }}
                            >
                              <ExternalLink className="h-4 w-4" />
                            </Button>
                          </div>
                        </div>
                      </CardHeader>
                      <CardContent className="pt-0">
                        {repo.description && (
                          <p className="text-sm text-muted-foreground mb-3">
                            {repo.description}
                          </p>
                        )}
                        <div className="flex items-center gap-4 text-xs text-muted-foreground">
                          <div className="flex items-center gap-1">
                            <Calendar className="h-3 w-3" />
                            Updated {formatDate(repo.updated_at)}
                          </div>
                          <div>Default: {repo.default_branch}</div>
                        </div>
                      </CardContent>
                    </Card>
                  ))
                )}
              </>
            )}
          </div>

          {selectRepositoryMutation.isPending && (
            <div className="flex items-center justify-center py-4">
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
              <span>Selecting repository...</span>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
