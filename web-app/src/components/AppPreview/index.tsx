import { ErrorBoundary } from "react-error-boundary";
import { Button } from "@/components/ui/shadcn/button";
import useApp from "@/hooks/api/useApp";
import useRunAppMutation from "@/hooks/api/useRunAppMutation";
import { Displays } from "@/components/AppPreview/Displays";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { useEffect } from "react";
import { toast } from "sonner";

type Props = {
  appPath64: string;
};

export default function AppPreview({ appPath64 }: Props) {
  const {
    mutate: runApp,
    isPending: isRunning,
    isError,
  } = useRunAppMutation(() => {});
  const { data: app, isPending } = useApp(appPath64);

  useEffect(() => {
    if (isError)
      toast.error("Error refreshing app. Check configuration and try again.");
  }, [isError]);

  const handleRun = () => runApp(appPath64);
  if (isPending)
    return (
      <div className="w-full h-full flex items-center justify-center">
        Loading...
      </div>
    );

  if (!app) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        Failed to load app. Check configuration and try again.
      </div>
    );
  }

  return (
    <div className="h-full w-full relative overflow-hidden">
      <Button
        className="absolute bottom-6 right-6 z-1"
        onClick={handleRun}
        disabled={isRunning}
        variant="default"
        content="icon"
      >
        {isRunning ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
      </Button>
      <div className="h-full w-full justify-center customScrollbar overflow-auto">
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
            <Displays displays={app.displays} data={app.data} />
          </ErrorBoundary>
        </div>
      </div>
    </div>
  );
}
