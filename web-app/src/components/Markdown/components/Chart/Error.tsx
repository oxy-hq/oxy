import { AlertCircle, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

type Props = {
  title: string;
  description: string;
  refetch: () => void;
};

const ChartError = ({ title, description, refetch }: Props) => {
  return (
    <div className='flex h-[400px] w-full flex-col items-center justify-center gap-4 rounded-md border border-destructive/20 bg-destructive/5 p-4'>
      <div className='flex items-center gap-2 text-destructive'>
        <AlertCircle className='h-5 w-5' />
        <span className='font-medium'>{title}</span>
      </div>
      <p className='max-w-md text-center text-muted-foreground text-sm'>{description}</p>
      <Button
        variant='outline'
        size='sm'
        onClick={() => refetch()}
        className='flex items-center gap-2'
      >
        <RefreshCw className='h-4 w-4' />
        Try Again
      </Button>
    </div>
  );
};

export default ChartError;
