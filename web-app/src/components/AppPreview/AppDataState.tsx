import type { UseQueryResult } from "@tanstack/react-query";
import ErrorAlert from "@/components/ui/ErrorAlert";
import IdeUnavailablePanel from "@/components/ui/IdeUnavailablePanel";
import { isIdeUnavailableError } from "@/libs/utils/ideHealth";
import type { AppData } from "@/types/app";

interface AppDataStateProps {
  appDataQueryResult: UseQueryResult<AppData, Error>;
  /** The last cached data is available and rendering below (ide down). */
  cachedAvailable?: boolean;
  /** The cached fallback is still being fetched — don't flash the panel. */
  cachedPending?: boolean;
}

const AppDataState = ({
  appDataQueryResult,
  cachedAvailable = false,
  cachedPending = false
}: AppDataStateProps) => {
  const { isError, error, data, refetch, isFetching } = appDataQueryResult;

  const renderErrorAlert = (message: string) => <ErrorAlert message={message} className='mb-2' />;

  // A data app runs against the ide's local DuckDB env; when the ide is
  // restarting that's a transient pause, not a misconfigured app.
  if (isError && isIdeUnavailableError(error)) {
    // The last cached data rendered below — a calm "stale" notice, not the panel.
    if (cachedAvailable) {
      return (
        <div className='mb-2 rounded-md border border-warning/30 bg-warning/5 px-3 py-2 text-muted-foreground text-xs'>
          Showing the last saved data — Oxygen Factory is restarting, so this dashboard isn&rsquo;t
          live.
        </div>
      );
    }
    // Still fetching the cache — let the loading state cover it, don't flash.
    if (cachedPending) {
      return null;
    }
    return (
      <IdeUnavailablePanel
        description='This dashboard needs Oxygen Factory, which is restarting. It will resume shortly.'
        onRetry={() => void refetch()}
        retrying={isFetching}
        className='mb-2'
      />
    );
  }

  if (isError && error) {
    return renderErrorAlert(error.message);
  }

  if (data?.error) {
    return renderErrorAlert(data.error);
  }

  return null;
};

export default AppDataState;
