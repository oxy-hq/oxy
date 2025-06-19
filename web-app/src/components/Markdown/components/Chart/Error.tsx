import { Button } from "@/components/ui/shadcn/button";
import { AlertCircle, RefreshCw } from "lucide-react";

type Props = {
  title: string;
  description: string;
  refetch: () => void;
};

const ChartError = ({ title, description, refetch }: Props) => {
  return (
    <div className="w-full h-[400px] flex flex-col items-center justify-center gap-4 p-4 border border-destructive/20 rounded-md bg-destructive/5">
      <div className="flex items-center gap-2 text-destructive">
        <AlertCircle className="h-5 w-5" />
        <span className="font-medium">{title}</span>
      </div>
      <p className="text-sm text-muted-foreground text-center max-w-md">
        {description}
      </p>
      <Button
        variant="outline"
        size="sm"
        onClick={() => refetch()}
        className="flex items-center gap-2"
      >
        <RefreshCw className="h-4 w-4" />
        Try Again
      </Button>
    </div>
  );
};

export default ChartError;
