import { AlertTriangle, RefreshCw, Settings } from "lucide-react";
import type React from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import ROUTES from "@/libs/utils/routes";

interface ConfigErrorScreenProps {
  onRetry?: () => void;
}

export const ConfigErrorScreen: React.FC<ConfigErrorScreenProps> = ({ onRetry }) => {
  const navigate = useNavigate();

  const handleGoToGitHubSettings = () => {
    navigate(ROUTES.SETTINGS.GITHUB);
  };

  return (
    <div className='flex min-h-screen w-full items-center justify-center bg-background'>
      <div className='max-w-md p-8 text-center'>
        <AlertTriangle className='mx-auto mb-6 h-16 w-16 text-destructive' />
        <h1 className='mb-4 font-bold text-2xl text-foreground'>Configuration Error</h1>
        <p className='mb-6 text-muted-foreground'>
          There was an error loading your project configuration. Please check your config.yml file
          and ensure it's properly formatted. You can also check your GitHub settings to ensure your
          repository is properly configured.
        </p>
        <div className='flex flex-col gap-3'>
          {onRetry && (
            <Button onClick={onRetry} className='inline-flex items-center gap-2'>
              <RefreshCw className='h-4 w-4' />
              Try Again
            </Button>
          )}
          <Button
            variant='outline'
            onClick={handleGoToGitHubSettings}
            className='inline-flex items-center gap-2'
          >
            <Settings className='h-4 w-4' />
            GitHub Settings
          </Button>
        </div>
      </div>
    </div>
  );
};
