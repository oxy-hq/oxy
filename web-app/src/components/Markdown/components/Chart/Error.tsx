import { RefreshCw } from "lucide-react";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Button } from "@/components/ui/shadcn/button";

type Props = {
  title: string;
  description: string;
  refetch: () => void;
};

const ChartError = ({ title, description, refetch }: Props) => {
  return (
    <ErrorAlert
      title={title}
      message={description}
      actions={
        <Button variant='outline' size='sm' onClick={() => refetch()}>
          <RefreshCw className='h-4 w-4' />
          Try Again
        </Button>
      }
    />
  );
};

export default ChartError;
