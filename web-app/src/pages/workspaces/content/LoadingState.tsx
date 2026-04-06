import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";

interface LoadingStateProps {
  error?: Error | null;
  onRetry?: () => void;
}

const LoadingState = ({ error, onRetry }: LoadingStateProps) => {
  if (error) {
    return (
      <div className='p-4'>
        <div className='mb-4 text-destructive'>Error loading workspaces</div>
        {onRetry && <Button onClick={onRetry}>Try Again</Button>}
      </div>
    );
  }

  return (
    <div className='flex h-64 w-full items-center justify-center'>
      <Spinner className='size-8' />
    </div>
  );
};

export default LoadingState;
