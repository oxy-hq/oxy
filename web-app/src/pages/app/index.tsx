import type React from "react";
import { useEffect, useMemo } from "react";
import { useParams } from "react-router-dom";
import { toast } from "sonner";
import AppPreview from "@/components/AppPreview";
import useRunAppMutation from "@/hooks/api/apps/useRunAppMutation";
import { decodeBase64 } from "@/libs/encoding";
import AppPageHeader from "./AppPageHeader";

// Main page
const AppPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const path = useMemo(() => decodeBase64(pathb64), [pathb64]);
  const { mutate: runApp, isPending: isRunning, isError } = useRunAppMutation(() => {});

  useEffect(() => {
    if (isError) toast.error("Error refreshing app. Check configuration and try again.");
  }, [isError]);

  const handleRun = () => runApp(pathb64);

  return (
    <div className='flex h-full w-full flex-col'>
      <AppPageHeader path={path} onRun={handleRun} isRunning={isRunning} />
      <div className='flex w-full flex-1 items-start justify-center overflow-auto'>
        <AppPreview appPath64={pathb64} runButton={false} />
      </div>
    </div>
  );
};

export default AppPage;
