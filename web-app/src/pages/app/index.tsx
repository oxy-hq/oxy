import React, { useEffect, useMemo } from "react";
import { useParams } from "react-router-dom";
import AppPageHeader from "./AppPageHeader";
import useRunAppMutation from "@/hooks/api/apps/useRunAppMutation";
import { toast } from "sonner";
import AppPreview from "@/components/AppPreview";

// Main page
const AppPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const {
    mutate: runApp,
    isPending: isRunning,
    isError,
  } = useRunAppMutation(() => {});

  useEffect(() => {
    if (isError)
      toast.error("Error refreshing app. Check configuration and try again.");
  }, [isError]);

  const handleRun = () => runApp(pathb64);

  return (
    <div className="w-full h-full flex flex-col">
      <AppPageHeader path={path} onRun={handleRun} isRunning={isRunning} />
      <div className="flex-1 w-full flex justify-center items-start overflow-auto">
        <AppPreview appPath64={pathb64} runButton={false} />
      </div>
    </div>
  );
};

export default AppPage;
