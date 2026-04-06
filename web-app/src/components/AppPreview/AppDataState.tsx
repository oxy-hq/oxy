import type { UseQueryResult } from "@tanstack/react-query";
import ErrorAlert from "@/components/ui/ErrorAlert";
import type { AppData } from "@/types/app";

interface AppDataStateProps {
  appDataQueryResult: UseQueryResult<AppData, Error>;
}

const AppDataState = ({ appDataQueryResult }: AppDataStateProps) => {
  const { isError, error, data } = appDataQueryResult;

  const renderErrorAlert = (message: string) => <ErrorAlert message={message} className='mb-2' />;

  if (isError && error) {
    return renderErrorAlert(error.message);
  }

  if (data?.error) {
    return renderErrorAlert(data.error);
  }

  return null;
};

export default AppDataState;
