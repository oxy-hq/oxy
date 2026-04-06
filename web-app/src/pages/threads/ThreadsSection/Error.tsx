import ErrorAlert from "@/components/ui/ErrorAlert";
import { Button } from "@/components/ui/shadcn/button";

const ErrorState = ({ error }: { error: Error }) => {
  return (
    <div className='flex flex-col items-center justify-center gap-4 p-6'>
      <ErrorAlert
        title='Error loading threads'
        message={error?.message || "Something went wrong"}
      />
      <Button variant='outline' onClick={() => window.location.reload()}>
        Try again
      </Button>
    </div>
  );
};

export default ErrorState;
