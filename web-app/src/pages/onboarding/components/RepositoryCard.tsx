import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Loader2 } from "lucide-react";
import { GitHubRepository } from "@/types/github";

interface RepositoryCardProps {
  repo: GitHubRepository;
  isSelecting: boolean;
  selectedRepo: GitHubRepository | null;
  onSelect: (repo: GitHubRepository) => void;
}

const RepositoryCard = ({
  repo,
  isSelecting,
  selectedRepo,
  onSelect,
}: RepositoryCardProps) => (
  <Card
    key={repo.id}
    className={`hover:shadow-lg transition-shadow cursor-pointer border-0 shadow-md ${
      selectedRepo?.id === repo.id ? "ring-2 ring-blue-500" : ""
    }`}
    onClick={() => !isSelecting && onSelect(repo)}
    data-testid="repository-card"
  >
    <CardHeader className="pb-3">
      <div className="flex items-start justify-between">
        <div className="flex-1 min-w-0">
          <CardTitle className="text-lg font-semibold truncate">
            {repo.name}
          </CardTitle>
          <CardDescription className="text-sm text-gray-600 dark:text-gray-400">
            {repo.full_name}
          </CardDescription>
        </div>
      </div>
    </CardHeader>

    <CardContent className="pt-0">
      {repo.description && (
        <p className="text-sm text-gray-600 dark:text-gray-400 mb-3 line-clamp-2">
          {repo.description}
        </p>
      )}

      <div className="flex items-center justify-between">
        <div className="text-xs text-gray-500 dark:text-gray-400">
          Default: <span className="font-mono">{repo.default_branch}</span>
        </div>

        <Button
          size="sm"
          disabled={isSelecting}
          className="bg-blue-600 hover:bg-blue-700"
        >
          {isSelecting && selectedRepo?.id === repo.id ? (
            <>
              <Loader2 className="mr-2 h-3 w-3 animate-spin" />
              Selecting...
            </>
          ) : (
            "Select"
          )}
        </Button>
      </div>
    </CardContent>
  </Card>
);

export default RepositoryCard;
