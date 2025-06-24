import React from "react";
import { Database, Table as TableIcon } from "lucide-react";
import { DatabaseInfo } from "@/types/database";

interface DatabaseInfoDisplayProps {
  database: DatabaseInfo;
}

export const DatabaseInfoDisplay: React.FC<DatabaseInfoDisplayProps> = ({
  database,
}) => (
  <div className="flex items-center gap-3">
    <Database className="h-4 w-4 text-muted-foreground flex-shrink-0" />
    <div className="min-w-0">
      <div className="font-medium truncate">{database.name}</div>
      <div className="text-xs text-muted-foreground truncate">
        {database.dialect} Database
      </div>
    </div>
  </div>
);

interface DatasetInfoDisplayProps {
  datasets: Record<string, unknown>;
}

export const DatasetInfoDisplay: React.FC<DatasetInfoDisplayProps> = ({
  datasets,
}) => {
  const datasetCount = Object.keys(datasets).length;
  const datasetKeys = Object.keys(datasets);

  const getDisplayText = () => {
    if (datasetCount === 0) return "All schemas";
    return `${datasetCount} ${datasetCount === 1 ? "Dataset" : "Datasets"}`;
  };

  return (
    <div className="flex items-center gap-2">
      <TableIcon className="h-4 w-4 text-muted-foreground flex-shrink-0" />
      <div className="min-w-0">
        <div className="text-sm font-medium">{getDisplayText()}</div>
        {datasetCount > 0 ? (
          <div className="text-xs text-muted-foreground truncate">
            {datasetKeys.slice(0, 2).join(", ")}
            {datasetCount > 2 && ` +${datasetCount - 2} more`}
          </div>
        ) : (
          <div className="text-xs text-muted-foreground">
            No specific datasets configured
          </div>
        )}
      </div>
    </div>
  );
};
