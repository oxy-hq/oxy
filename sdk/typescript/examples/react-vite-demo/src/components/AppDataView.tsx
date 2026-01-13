import { useState } from "react";
import { useOxy, type DataContainer, type QueryResult } from "@oxy-hq/sdk";
import DataPreviewModal from "./DataPreviewModal";

interface AppDataViewProps {
  appData: DataContainer | null;
}

interface TableData {
  columns: string[];
  rows: unknown[][];
  total_rows?: number;
}

interface ActivePreview {
  name: string;
  data: TableData;
}

export default function AppDataView({ appData }: AppDataViewProps) {
  const { sdk } = useOxy();
  const [loadingDataset, setLoadingDataset] = useState<string | null>(null);
  const [error, setError] = useState<{ name: string; message: string } | null>(
    null,
  );
  const [activePreview, setActivePreview] = useState<ActivePreview | null>(
    null,
  );

  if (!sdk) {
    return (
      <div className="empty-state">
        <p>SDK not ready</p>
      </div>
    );
  }

  if (!appData) {
    return (
      <div className="empty-state">
        <p>Select an app to view its data</p>
      </div>
    );
  }

  const entries = Object.entries(appData);

  if (entries.length === 0) {
    return (
      <div className="empty-state">
        <p className="empty-text">No data available</p>
      </div>
    );
  }

  const handlePreview = async (name: string) => {
    setLoadingDataset(name);
    setError(null);

    try {
      // Query the data directly using the table name (already registered by SDK)
      const result: QueryResult = await sdk.query(
        `SELECT * FROM ${name} LIMIT 100`,
      );

      const tableData: TableData = {
        columns: result.columns,
        rows: result.rows,
        total_rows: result.rowCount,
      };

      setActivePreview({ name, data: tableData });
      setLoadingDataset(null);
    } catch (err) {
      setError({
        name,
        message: (err as Error).message || "Failed to load data",
      });
      setLoadingDataset(null);
    }
  };

  const handleCloseModal = () => {
    setActivePreview(null);
  };

  return (
    <div className="app-data">
      <div className="datasets-grid">
        {entries.map(([name, ref]) => {
          const isLoading = loadingDataset === name;
          const hasError = error?.name === name;

          return (
            <div key={name} className="dataset-card">
              <div className="dataset-header">
                <div className="dataset-name">{name}</div>
                <div className="dataset-path">
                  <code>{ref.file_path}</code>
                </div>
                <button
                  onClick={() => handlePreview(name)}
                  className="btn-preview"
                  disabled={isLoading}
                >
                  {isLoading ? "⏳ Loading..." : "👁 Preview"}
                </button>
              </div>

              {hasError && (
                <div className="preview-error">
                  <strong>Error:</strong> {error.message}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {activePreview && (
        <DataPreviewModal
          name={activePreview.name}
          data={activePreview.data}
          onClose={handleCloseModal}
        />
      )}
    </div>
  );
}
