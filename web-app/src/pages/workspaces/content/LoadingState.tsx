import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface LoadingStateProps {
  error?: Error | null;
  onRetry?: () => void;
}

const LoadingState = ({ error, onRetry }: LoadingStateProps) => {
  if (error) {
    return (
      <div className='p-4'>
        <div className='mb-4 text-red-600'>Error loading workspaces</div>
        {onRetry && <Button onClick={onRetry}>Try Again</Button>}
      </div>
    );
  }

  return (
    <div className='flex h-64 w-full items-center justify-center'>
      <Loader2 className='h-8 w-8 animate-spin' />
    </div>
  );
};

export default LoadingState;
