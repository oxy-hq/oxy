import React from "react";
import { AlertTriangle, RefreshCw, Settings } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { useNavigate } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";

interface ConfigErrorScreenProps {
  onRetry?: () => void;
}

export const ConfigErrorScreen: React.FC<ConfigErrorScreenProps> = ({
  onRetry,
}) => {
  const navigate = useNavigate();

  const handleGoToGitHubSettings = () => {
    navigate(ROUTES.SETTINGS.GITHUB);
  };

  return (
    <div className="flex w-full items-center justify-center min-h-screen bg-background">
      <div className="text-center p-8 max-w-md">
        <AlertTriangle className="mx-auto h-16 w-16 text-destructive mb-6" />
        <h1 className="text-2xl font-bold text-foreground mb-4">
          Configuration Error
        </h1>
        <p className="text-muted-foreground mb-6">
          There was an error loading your project configuration. Please check
          your config.yml file and ensure it's properly formatted. You can also
          check your GitHub settings to ensure your repository is properly
          configured.
        </p>
        <div className="flex flex-col gap-3">
          {onRetry && (
            <Button
              onClick={onRetry}
              className="inline-flex items-center gap-2"
            >
              <RefreshCw className="h-4 w-4" />
              Try Again
            </Button>
          )}
          <Button
            variant="outline"
            onClick={handleGoToGitHubSettings}
            className="inline-flex items-center gap-2"
          >
            <Settings className="h-4 w-4" />
            GitHub Settings
          </Button>
        </div>
      </div>
    </div>
  );
};
