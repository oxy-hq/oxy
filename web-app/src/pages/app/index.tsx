import React, { useEffect, useMemo } from "react";
import { useParams } from "react-router-dom";
import AppPageHeader from "./AppPageHeader";
import useApp from "@/hooks/api/apps/useApp";
import useRunAppMutation from "@/hooks/api/apps/useRunAppMutation";
import { Displays } from "../../components/AppPreview/Displays";
import { toast } from "sonner";
import { ErrorBoundary } from "react-error-boundary";
import PageSkeleton from "@/components/PageSkeleton";

// Main page
const AppPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const {
    mutate: runApp,
    isPending: isRunning,
    isError,
  } = useRunAppMutation(() => {});
  const { data: app, isPending } = useApp(pathb64);

  useEffect(() => {
    if (isError)
      toast.error("Error refreshing app. Check configuration and try again.");
  }, [isError]);

  const handleRun = () => runApp(pathb64);

  if (isPending) return <PageSkeleton />;

  if (!app) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <div className="text-red-600">
          Failed to load app. Check configuration and try again.
        </div>
      </div>
    );
  }

  return (
    <div className="w-full h-full flex flex-col">
      <AppPageHeader path={path} onRun={handleRun} isRunning={isRunning} />
      <div className="flex-1 w-full flex justify-center items-start overflow-auto customScrollbar">
        <div className="p-16 max-w-200 w-full">
          <ErrorBoundary
            resetKeys={[app]}
            fallback={
              <div className="text-red-600">
                Failed to render app. Refresh the data or check for
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
};

export default AppPage;
