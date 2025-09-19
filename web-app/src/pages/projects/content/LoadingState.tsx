import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface LoadingStateProps {
  error?: Error | null;
  onRetry?: () => void;
}

const LoadingState = ({ error, onRetry }: LoadingStateProps) => {
  if (error) {
    return (
      <div className="p-4">
        <div className="text-red-600 mb-4">Error loading projects</div>
        {onRetry && <Button onClick={onRetry}>Try Again</Button>}
      </div>
    );
  }

  return (
    <div className="flex items-center justify-center h-64 w-full">
      <Loader2 className="h-8 w-8 animate-spin" />
    </div>
  );
};

export default LoadingState;
