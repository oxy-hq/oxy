import { ErrorBoundary } from "react-error-boundary";
import { Button } from "@/components/ui/shadcn/button";
import useApp from "@/hooks/api/useApp";
import useRunAppMutation from "@/hooks/api/useRunAppMutation";
import { Displays } from "@/pages/app/Displays";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { useEffect } from "react";
import { toast } from "sonner";

type Props = {
  appPath: string;
};

export default function AppPreview({ appPath }: Props) {
  const {
    mutate: runApp,
    isPending: isRunning,
    isError,
  } = useRunAppMutation(() => {});
  const { data: app, isPending } = useApp(appPath);

  useEffect(() => {
    if (isError)
      toast.error("Error refreshing app. Check configuration and try again.");
  }, [isError]);

  const handleRun = () => runApp(appPath);
  if (isPending)
    return (
      <div className="w-full h-full flex items-center justify-center">
        Loading...
      </div>
    );

  return (
    <>
      <Button
        onClick={handleRun}
        disabled={isRunning}
        variant="default"
        content="icon"
        className="absolute w-14 h-14 bottom-4 right-4 z-1 rounded-full break-words"
      >
        {isRunning ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
      </Button>
      <div className="p-8 max-w-200 w-full">
        <ErrorBoundary
          resetKeys={[app]}
          fallback={
            <div className="text-red-600">
              Failed to render app review. Refresh the data or check for
              configuration errors.
            </div>
          }
        >
          <Displays displays={app!.displays} data={app!.data} />
        </ErrorBoundary>
      </div>
    </>
  );
}
