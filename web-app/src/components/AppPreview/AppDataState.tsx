import { CircleAlert } from "lucide-react";
import { Alert, AlertDescription } from "../ui/shadcn/alert";
import { UseQueryResult } from "@tanstack/react-query";
import { AppData } from "@/types/app";

interface AppDataStateProps {
  appDataQueryResult: UseQueryResult<AppData, Error>;
}

const AppDataState = ({ appDataQueryResult }: AppDataStateProps) => {
  const { isError, error, data } = appDataQueryResult;

  const renderErrorAlert = (message: string) => (
    <Alert variant="destructive" className="mb-2">
      <CircleAlert className="h-4 w-4" />
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
