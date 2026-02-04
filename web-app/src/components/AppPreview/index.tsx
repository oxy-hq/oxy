import { LoaderCircle, RefreshCw } from "lucide-react";
import { useEffect } from "react";
import { toast } from "sonner";
import { Displays } from "@/components/AppPreview/Displays";
import { Button } from "@/components/ui/shadcn/button";
import useAppData, { useAppDisplays } from "@/hooks/api/apps/useApp";
import useRunAppMutation from "@/hooks/api/apps/useRunAppMutation";
import AppDataState from "./AppDataState";

type Props = {
  appPath64: string;
  runButton?: boolean;
};

export default function AppPreview({ appPath64, runButton = true }: Props) {
  const { mutate: runApp, isPending: isRunning, isError } = useRunAppMutation(() => {});
  const appDataQueryResult = useAppData(appPath64);
  const { data: appDisplay } = useAppDisplays(appPath64);

  useEffect(() => {
    if (isError) toast.error("Error refreshing app. Check configuration and try again.");
  }, [isError]);

  const handleRun = () => runApp(appPath64);

  return (
    <div className='relative h-full w-full overflow-hidden' data-testid='app-preview'>
      {runButton && (
        <Button
          className='absolute right-6 bottom-6 z-1'
          onClick={handleRun}
          disabled={isRunning || appDataQueryResult.isPending}
          variant='default'
          content='icon'
        >
          {isRunning ? <LoaderCircle className='animate-spin' /> : <RefreshCw />}
        </Button>
      )}

      <div className='customScrollbar h-full w-full overflow-auto'>
        <div className='mr-auto ml-auto w-full max-w-200 p-2'>
          <AppDataState appDataQueryResult={appDataQueryResult} />
          <Displays displays={appDisplay?.displays || []} data={appDataQueryResult.data?.data} />
        </div>
      </div>
    </div>
  );
}
