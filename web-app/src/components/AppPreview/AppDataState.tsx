import type { UseQueryResult } from "@tanstack/react-query";
import type { AppData } from "@/types/app";
import { ErrorAlert, ErrorAlertMessage } from "./ErrorAlert";

interface AppDataStateProps {
  appDataQueryResult: UseQueryResult<AppData, Error>;
}

const AppDataState = ({ appDataQueryResult }: AppDataStateProps) => {
  const { isError, error, data } = appDataQueryResult;

  const renderErrorAlert = (message: string) => (
    <ErrorAlert className='mb-2'>
      <ErrorAlertMessage>{message}</ErrorAlertMessage>
    </ErrorAlert>
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
