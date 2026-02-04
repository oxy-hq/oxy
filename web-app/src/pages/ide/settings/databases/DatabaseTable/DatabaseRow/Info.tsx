import {
  Cloud,
  Cylinder,
  HardDrive,
  Database as LucideDatabase,
  Snowflake,
  Zap
} from "lucide-react";
import type React from "react";
import type { DatabaseInfo as Database } from "@/types/database";

const getDatabaseIcon = (dialect: string) => {
  const iconProps = "h-4 w-4 text-muted-foreground flex-shrink-0";

  switch (dialect.toLowerCase()) {
    case "bigquery":
      return <Cloud className={iconProps} />;
    case "postgres":
    case "postgresql":
      return <Cylinder className={iconProps} />;
    case "mysql":
      return <HardDrive className={iconProps} />;
    case "snowflake":
      return <Snowflake className={iconProps} />;
    case "clickhouse":
      return <Zap className={iconProps} />;
    default:
      return <LucideDatabase className={iconProps} />;
  }
};

interface DatabaseInfoDisplayProps {
  database: Database;
}

export const DatabaseInfo: React.FC<DatabaseInfoDisplayProps> = ({ database }) => (
  <div className='flex items-center gap-2'>
    {getDatabaseIcon(database.dialect)}
    <div className='truncate'>{database.name}</div>
  </div>
);

interface DatasetInfoDisplayProps {
  datasets: Record<string, unknown>;
}

export const DatasetInfo: React.FC<DatasetInfoDisplayProps> = ({ datasets }) => {
  const datasetCount = Object.keys(datasets).length;
  const datasetKeys = Object.keys(datasets);

  return (
    <div className='flex items-center gap-2'>
      <div className='min-w-0'>
        {datasetCount > 0 ? (
          <div className='truncate'>
            {datasetKeys.slice(0, 2).join(", ")}
            {datasetCount > 2 && ` +${datasetCount - 2} more`}
          </div>
        ) : (
          <div className='text-muted-foreground'>No specific datasets configured</div>
        )}
      </div>
    </div>
  );
};
