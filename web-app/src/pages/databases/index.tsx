import React from "react";
import PageHeader from "@/components/PageHeader";
import { useSidebar } from "@/components/ui/shadcn/sidebar";
import DatabaseTable, { EmbeddingsManagement } from "@/components/Database";
import useDatabases from "@/hooks/api/useDatabases";
import { AlertCircle } from "lucide-react";

const DatabaseManagement: React.FC = () => {
  const { open } = useSidebar();

  const {
    data: databases = [],
    isLoading: isLoadingDatabases,
    error: databasesError,
  } = useDatabases(true);

  return (
    <div className="flex flex-col h-full">
      {!open && <PageHeader />}

      <div className="flex-1 p-6">
        <div className="max-w-6xl mx-auto">
          <div className="flex justify-between items-center mb-6">
            <div>
              <h1 className="text-2xl font-semibold">Data Management</h1>
              <p className="text-muted-foreground mt-1">
                Sync databases and manage embeddings for your data
              </p>
            </div>
          </div>

          {databasesError ? (
            <div className="mb-6 px-4 py-3 rounded border flex items-center text-sm bg-red-100 border-red-300 text-red-800">
              <AlertCircle className="h-4 w-4 mr-2 text-red-500" />
              <span className="font-semibold mr-1">Error:</span>
              <span>Failed to load databases</span>
            </div>
          ) : null}

          <div className="space-y-6">
            <div>
              <h2 className="text-lg font-semibold mb-4">
                Database Connections
              </h2>
              <DatabaseTable
                databases={databases}
                loading={isLoadingDatabases}
              />
            </div>

            <div>
              <h2 className="text-lg font-semibold mb-4">AI & Embeddings</h2>
              <EmbeddingsManagement />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default DatabaseManagement;
