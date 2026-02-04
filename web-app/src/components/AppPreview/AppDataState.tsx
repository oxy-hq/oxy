import type { UseQueryResult } from "@tanstack/react-query";
import { CircleAlert } from "lucide-react";
import type { AppData } from "@/types/app";
import { Alert, AlertDescription } from "../ui/shadcn/alert";

interface AppDataStateProps {
  appDataQueryResult: UseQueryResult<AppData, Error>;
}

const AppDataState = ({ appDataQueryResult }: AppDataStateProps) => {
  const { isError, error, data } = appDataQueryResult;

  const renderErrorAlert = (message: string) => (
    <Alert variant='destructive' className='mb-2'>
      <CircleAlert className='h-4 w-4' />
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  );

  if (isError && error) {
    return renderErrorAlert(error.message);
  }

  if (data?.error) {
    return renderErrorAlert(data.error);
  }

  return null;
};

export default AppDataState;
