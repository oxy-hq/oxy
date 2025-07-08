import { GitBranch } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { GitHubRepository, RepositorySyncStatus } from "@/types/github";

// Constants
const SYNC_STATUS_STYLES = {
  synced:
    "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400",
  syncing:
    "bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400",
  idle: "bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400",
  error: "bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400",
} as const;

// Helper functions
const getSyncStatusStyle = (status: RepositorySyncStatus | null): string => {
  if (!status) return SYNC_STATUS_STYLES.idle;
  return SYNC_STATUS_STYLES[status] || SYNC_STATUS_STYLES.error;
};

const getSyncStatusLabel = (status: RepositorySyncStatus | null): string => {
  return status || "idle";
};

interface ProjectCardProps {
  project: {
    repository?: GitHubRepository;
    sync_status: RepositorySyncStatus | null;
  };
}

const ProjectCard = ({ project }: ProjectCardProps) => (
  <Card className="shadow-xl border-0 mb-8">
    <CardHeader>
      <CardTitle className="flex items-center space-x-2">
        <GitBranch className="h-5 w-5 text-blue-600" />
        <span>{project.repository?.name}</span>
      </CardTitle>
      <CardDescription>{project.repository?.full_name}</CardDescription>
    </CardHeader>

    <CardContent className="space-y-4">
      {project.repository?.description && (
        <p className="text-gray-600 dark:text-gray-400">
          {project.repository.description}
        </p>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="flex items-center space-x-2 text-sm">
          <GitBranch className="h-4 w-4 text-gray-500" />
          <span className="font-medium">Default Branch:</span>
          <span className="font-mono text-xs bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
            {project.repository?.default_branch}
          </span>
        </div>
      </div>

      <div className="flex items-center justify-between pt-4 border-t">
        <div className="text-sm text-gray-600 dark:text-gray-400">
          <span className="font-medium">Sync Status:</span>
          <span
            className={`ml-2 px-2 py-1 rounded-full text-xs font-medium ${getSyncStatusStyle(
              project.sync_status,
            )}`}
          >
            {getSyncStatusLabel(project.sync_status)}
          </span>
        </div>
      </div>
    </CardContent>
  </Card>
);

export default ProjectCard;
