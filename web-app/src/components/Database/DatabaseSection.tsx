import { useState, useCallback, memo, useRef } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/shadcn/toggle-group";
import {
  Loader2,
  Database,
  DatabaseZap,
  RefreshCw,
  Hammer,
  AlertCircle,
  ChevronDown,
  ChevronUp,
  Info,
} from "lucide-react";
import { useDatabaseSync } from "@/hooks/api/useDatabaseSync";
import { useDataBuild as useDataBuild } from "@/hooks/api/useDataBuild";
import useDatabases from "@/hooks/api/useDatabases";
import { DatabaseInfo } from "@/types/database";
import DatabaseDropdown from "./DatabaseDropdown";
import DatasetDropdown from "./DatasetDropdown";

const ToggleGroupItemClasses =
  "data-[state=on]:border data-[state=on]:border-blue-500 data-[state=on]:bg-blue-500 data-[state=on]:text-white hover:bg-blue-500/20 hover:text-blue-300 hover:border-blue-400/50 transition-colors border-gray-600 rounded-md text-gray-400";

function SectionAlert({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="max-w-[800px] mx-auto mb-3 px-3 py-2 rounded border flex items-center text-sm bg-blue-100 border-blue-300 text-blue-800"
      role="alert"
    >
      <Info className="h-4 w-4 mr-2 text-blue-500" />
      <span className="font-semibold mr-1">{title}:</span>
      <span>{children}</span>
    </div>
  );
}

const DataSection = memo(() => {
  const [isExpanded, setIsExpanded] = useState(false);
  const [selectedDatabase, setSelectedDatabase] = useState<DatabaseInfo | null>(
    null,
  );
  const [selectedDatasets, setSelectedDatasets] = useState<string[]>([]);
  const [mode, setMode] = useState<string>("sync");
  const [message, setMessage] = useState<string | null>(null);
  const [messageType, setMessageType] = useState<"success" | "error" | null>(
    null,
  );
  const messageTimeout = useRef<NodeJS.Timeout | null>(null);

  const {
    data: databases = [],
    isLoading: isLoadingDatabases,
    error: databasesError,
  } = useDatabases(isExpanded);

  const toggleExpanded = () => {
    setIsExpanded(!isExpanded);
  };

  const syncMutation = useDatabaseSync();
  const buildMutation = useDataBuild();

  const showMessage = (msg: string, type: "success" | "error") => {
    setMessage(msg);
    setMessageType(type);
    if (messageTimeout.current) clearTimeout(messageTimeout.current);
    messageTimeout.current = setTimeout(() => {
      setMessage(null);
      setMessageType(null);
    }, 3500);
  };

  const handleSync = useCallback(async () => {
    try {
      const result = await syncMutation.mutateAsync({
        database: selectedDatabase?.name,
        options: {
          ...(selectedDatasets.length > 0 && { datasets: selectedDatasets }),
        },
      });

      if (result.success) {
        showMessage(
          result.message || "Database synced successfully",
          "success",
        );
      } else {
        showMessage(result.message || "Failed to sync database", "error");
      }
    } catch (err) {
      console.error("Database sync error:", err);
      showMessage("An error occurred while syncing the database", "error");
    }
  }, [syncMutation, selectedDatabase, selectedDatasets]);

  const handleBuild = useCallback(async () => {
    try {
      const result = await buildMutation.mutateAsync();

      if (result.success) {
        showMessage(
          result.message || "Embeddings built successfully",
          "success",
        );
      } else {
        showMessage(result.message || "Failed to build embeddings", "error");
      }
    } catch (err) {
      console.error("Embeddings build error:", err);
      showMessage("An error occurred while building embeddings", "error");
    }
  }, [buildMutation]);

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    switch (mode) {
      case "sync":
        await handleSync();
        break;
      case "build":
        await handleBuild();
        break;
    }
  };

  const disabled = () => {
    if (syncMutation.isPending || buildMutation.isPending) return true;
    switch (mode) {
      case "sync":
        return !selectedDatabase;
      case "build":
        return false; // Build doesn't require database selection
    }
    return false;
  };

  const submitIcon = mode === "build" ? <Hammer /> : <RefreshCw />;

  // Error state
  if (databasesError && isExpanded) {
    return (
      <div className="w-full max-w-[800px] flex flex-col shadow-sm rounded-md border-2 mx-auto bg-secondary">
        {/* Collapsible Header */}
        <div
          className="flex items-center gap-2 p-2 cursor-pointer hover:bg-sidebar-accent transition-colors"
          onClick={toggleExpanded}
        >
          <Database className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium text-muted-foreground">
            Data Management
          </span>
          <div className="ml-auto">
            <ChevronUp className="h-4 w-4 text-muted-foreground" />
          </div>
        </div>

        <div className="flex items-center gap-2 text-destructive p-4">
          <AlertCircle className="h-4 w-4" />
          <span className="text-sm">Failed to load databases</span>
        </div>
      </div>
    );
  }

  // Loading state
  if (isLoadingDatabases && isExpanded) {
    return (
      <div className="w-full max-w-[800px] flex flex-col shadow-sm rounded-md border-2 mx-auto bg-secondary">
        {/* Collapsible Header */}
        <div
          className="flex items-center gap-2 p-2 cursor-pointer hover:bg-sidebar-accent transition-colors"
          onClick={toggleExpanded}
        >
          <Database className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium text-muted-foreground">
            Data Management
          </span>
          <div className="ml-auto">
            <ChevronUp className="h-4 w-4 text-muted-foreground" />
          </div>
        </div>

        <div className="flex items-center justify-center p-4">
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          <span className="ml-2 text-sm text-muted-foreground">
            Loading databases...
          </span>
        </div>
      </div>
    );
  }

  return (
    <>
      {message && (
        <SectionAlert title={messageType === "success" ? "Success" : "Error"}>
          {message}
        </SectionAlert>
      )}
      <div className="w-full max-w-[800px] flex flex-col shadow-sm rounded-md border-2 mx-auto bg-secondary relative">
        {/* Collapsible Header */}
        <div
          className="flex items-center gap-2 p-2 cursor-pointer hover:bg-sidebar-accent transition-colors"
          onClick={toggleExpanded}
        >
          <DatabaseZap className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium text-muted-foreground">
            Data Management
          </span>
          <div className="ml-auto">
            {isExpanded ? (
              <ChevronUp className="h-4 w-4 text-muted-foreground" />
            ) : (
              <ChevronDown className="h-4 w-4 text-muted-foreground" />
            )}
          </div>
        </div>

        {/* Expanded Content */}
        {isExpanded && (
          <form onSubmit={handleFormSubmit} className="flex p-3 flex-col gap-2">
            <div className="flex flex-col gap-3 lg:flex-row lg:justify-between lg:items-center">
              <div className="flex items-center justify-center lg:justify-start">
                <ToggleGroup
                  size="sm"
                  type="single"
                  value={mode}
                  className="gap-1 p-1 bg-sidebar-background text-accent-main-000 rounded-md"
                  onValueChange={(value) => {
                    if (value) {
                      setMode(value);
                    }
                  }}
                >
                  <ToggleGroupItem
                    size="sm"
                    value="sync"
                    className={ToggleGroupItemClasses}
                  >
                    <RefreshCw className="h-4 w-4" />
                    <span className="whitespace-nowrap">Sync Database</span>
                  </ToggleGroupItem>
                  <ToggleGroupItem
                    size="sm"
                    value="build"
                    className={ToggleGroupItemClasses}
                  >
                    <Hammer className="h-4 w-4" />
                    <span className="whitespace-nowrap">Build Embeddings</span>
                  </ToggleGroupItem>
                </ToggleGroup>
              </div>

              <div className="flex gap-2 items-center flex-wrap lg:flex-nowrap">
                {mode === "sync" && (
                  <>
                    <DatabaseDropdown
                      onSelect={setSelectedDatabase}
                      database={selectedDatabase}
                      databases={databases}
                      isLoading={isLoadingDatabases}
                    />
                    {selectedDatabase &&
                      Object.keys(selectedDatabase.datasets).length > 0 && (
                        <DatasetDropdown
                          onSelect={setSelectedDatasets}
                          selectedDatasets={selectedDatasets}
                          availableDatasets={selectedDatabase.datasets}
                        />
                      )}
                  </>
                )}

                <Button
                  disabled={disabled()}
                  type="submit"
                  className="whitespace-nowrap"
                >
                  {syncMutation.isPending || buildMutation.isPending ? (
                    <Loader2 className="animate-spin h-4 w-4" />
                  ) : (
                    submitIcon
                  )}
                </Button>
              </div>
            </div>

            {databases.length === 0 && (
              <div className="text-center py-3 text-muted-foreground">
                <DatabaseZap className="h-6 w-6 mx-auto mb-1 opacity-40" />
                <p className="text-xs">No databases configured</p>
              </div>
            )}
          </form>
        )}
      </div>
    </>
  );
});

DataSection.displayName = "DataSection";

export default DataSection;
