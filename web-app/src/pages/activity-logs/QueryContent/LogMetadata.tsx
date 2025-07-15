import { LogItem } from "@/types/logs";
import QueryMetadata from "./QueryMetadata";

const LogMetadata = ({ log }: { log: LogItem }) => {
  if (log.log && typeof log.log === "object" && "queries" in log.log) {
    const logData = log.log as Record<string, unknown>;
    const queries = logData.queries;

    if (Array.isArray(queries) && queries.length > 0) {
      const firstQuery = queries[0];
      return <QueryMetadata queryItem={firstQuery} />;
    }
  }

  return <div className="text-sm text-muted-foreground">No metadata</div>;
};

export default LogMetadata;
