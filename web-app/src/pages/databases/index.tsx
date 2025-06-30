import { AlertCircle, HardDrive } from "lucide-react";
import DatabaseTable, { EmbeddingsManagement } from "@/components/Database";
import useDatabases from "@/hooks/api/useDatabases";

const DatabaseManagement = () => {
  const {
    data: databases = [],
    isLoading: isLoadingDatabases,
    error: databasesError,
  } = useDatabases(true);

  if (isLoadingDatabases) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 p-6">
          <div className="max-w-4xl mx-auto">
            <div className="flex items-center justify-center h-64">
              <div className="text-lg">Loading databases...</div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 p-6">
        <div className="max-w-4xl mx-auto">
          <div className="flex items-center space-x-3 mb-6">
            <HardDrive className="h-6 w-6" />
            <div>
              <h1 className="text-xl font-semibold">Databases</h1>
              <p className="text-sm text-muted-foreground">
                Sync databases and manage embeddings for your data
              </p>
            </div>
          </div>

          {!!databasesError && (
            <div className="mb-6">
              <div className="flex items-center space-x-2 text-red-600">
                <AlertCircle className="h-5 w-5" />
                <span>
                  Error loading databases:{" "}
                  {typeof databasesError === "object" &&
                  databasesError &&
                  "message" in databasesError
                    ? (databasesError as { message?: string }).message
                    : String(databasesError)}
                </span>
              </div>
            </div>
          )}

          <div className="space-y-8">
            {/* Database Connections Section */}
            <div>
              <h2 className="text-lg font-semibold mb-4">
                Database Connections
              </h2>
              <div className="border rounded-lg">
                <DatabaseTable
                  databases={databases}
                  loading={isLoadingDatabases}
                />
              </div>
            </div>

            {/* AI & Embeddings Section */}
            <div>
              <h2 className="text-lg font-semibold mb-4">AI & Embeddings</h2>
              <div className="border rounded-lg">
                <EmbeddingsManagement />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default DatabaseManagement;
