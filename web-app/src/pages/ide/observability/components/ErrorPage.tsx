import { AlertCircle, ArrowLeft } from "lucide-react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";

interface ErrorPageProps {
  message: string;
  description: string;
}

export function ErrorPage({ message, description }: ErrorPageProps) {
  return (
    <div className='flex h-full flex-col items-center justify-center gap-4'>
      <AlertCircle className='h-12 w-12 text-destructive' />
      <div className='font-medium text-lg'>{message}</div>
      <div className='text-muted-foreground'>{description}</div>
      <Button variant='outline' asChild>
        <Link to='/traces'>
          <ArrowLeft className='mr-2 h-4 w-4' />
          Back to Traces
        </Link>
      </Button>
    </div>
  );
}
