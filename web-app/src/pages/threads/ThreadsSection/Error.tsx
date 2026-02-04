import { Button } from "@/components/ui/shadcn/button";

const ErrorState = ({ error }: { error: Error }) => {
  return (
    <div className='flex flex-col items-center justify-center gap-4 p-6'>
      <div className='text-center text-red-500'>
        <p className='font-semibold text-lg'>Error loading threads</p>
        <p className='text-muted-foreground text-sm'>{error?.message || "Something went wrong"}</p>
      </div>
      <Button variant='outline' onClick={() => window.location.reload()}>
        Try again
      </Button>
    </div>
  );
};

export default ErrorState;
