import { useCallback, useRef } from "react";
import type { editor } from "monaco-editor";
import { Play, Save, X, Plus, Loader2, Code } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import useDatabaseClient from "@/stores/useDatabaseClient";
import { DatabaseService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { toast } from "sonner";
import { format as formatSQL } from "sql-formatter";
import DatabaseSelector from "@/components/sql/DatabaseSelector";
import { BaseMonacoEditor, useMonacoSetup } from "@/components/MonacoEditor";

interface QueryEditorProps {
  onSave: () => void;
}

export default function QueryEditor({ onSave }: QueryEditorProps) {
  const { project, branchName } = useCurrentProjectBranch();
  const {
    tabs,
    activeTabId,
    addTab,
    updateTab,
    removeTab,
    setActiveTab,
    setTabExecuting,
    setTabResults,
    setTabError,
  } = useDatabaseClient();

  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);

  const activeTab = tabs.find((t) => t.id === activeTabId);

  const handleRunQuery = useCallback(async () => {
    if (!activeTab || !activeTab.content.trim()) {
      toast.error("No query to execute");
      return;
    }

    if (!activeTab.selectedDatabase) {
      toast.error("Please select a database");
      return;
    }

    setTabExecuting(activeTab.id, true);
    const startTime = performance.now();

    try {
      const response = await DatabaseService.executeSqlQuery(
        project.id,
        branchName,
        activeTab.content,
        activeTab.selectedDatabase,
      );

      const executionTime = performance.now() - startTime;

      // Handle parquet response (file reference)
      if (response && typeof response === "object" && "file_name" in response) {
        toast.success(
          `Query executed in ${executionTime.toFixed(0)}ms (results saved to file)`,
        );
        setTabResults(activeTab.id, {
          result: [],
          resultFile: (response as { file_name: string }).file_name,
          executionTime,
        });
        return;
      }

      setTabResults(activeTab.id, {
        result: response as string[][],
        resultFile: undefined,
        executionTime,
      });

      toast.success(`Query executed in ${executionTime.toFixed(0)}ms`);
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : "Query execution failed";
      setTabError(activeTab.id, errorMessage);
      toast.error(errorMessage);
    }
  }, [
    activeTab,
    project.id,
    branchName,
    setTabExecuting,
    setTabResults,
    setTabError,
  ]);

  useMonacoSetup({ onSave, onExecute: handleRunQuery });

  const handleNewTab = () => {
    const result = addTab({ selectedDatabase: activeTab?.selectedDatabase });
    if (!result.success) {
      toast.error(result.error);
    }
  };

  const handleCloseTab = (e: React.MouseEvent, tabId: string) => {
    e.stopPropagation();
    removeTab(tabId);
  };

  const handleContentChange = (value: string) => {
    if (activeTab) {
      updateTab(activeTab.id, { content: value, isDirty: true });
    }
  };

  const handleFormatSQL = useCallback(() => {
    if (!activeTab || !activeTab.content.trim()) {
      toast.error("No query to format");
      return;
    }

    try {
      const formatted = formatSQL(activeTab.content, {
        language: "sql",
        keywordCase: "upper",
        indentStyle: "standard",
        logicalOperatorNewline: "before",
      });
      updateTab(activeTab.id, { content: formatted, isDirty: true });
      toast.success("SQL formatted");
    } catch {
      toast.error("Failed to format SQL");
    }
  }, [activeTab, updateTab]);

  const handleEditorMount = (editor: editor.IStandaloneCodeEditor) => {
    editorRef.current = editor;
  };

  return (
    <div className="h-full flex flex-col bg-background">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-2 py-1 border-b bg-muted/30">
        <div className="flex items-center gap-1">
          {/* Database Selector */}
          <div className="mr-2">
            <DatabaseSelector
              onSelect={(db) =>
                activeTab && updateTab(activeTab.id, { selectedDatabase: db })
              }
              database={activeTab?.selectedDatabase ?? null}
            />
          </div>

          <Button
            variant="ghost"
            size="sm"
            onClick={handleRunQuery}
            disabled={
              !activeTab ||
              activeTab.isExecuting ||
              !activeTab?.selectedDatabase
            }
            className="h-7 px-2"
          >
            {activeTab?.isExecuting ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Play className="h-4 w-4" />
            )}
            <span className="ml-1">Run</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={onSave}
            disabled={!activeTab}
            className="h-7 px-2"
          >
            <Save className="h-4 w-4" />
            <span className="ml-1">Save</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={handleFormatSQL}
            disabled={!activeTab}
            className="h-7 px-2"
          >
            <Code className="h-4 w-4" />
            <span className="ml-1">Format</span>
          </Button>
        </div>

        <div className="text-xs text-muted-foreground">
          {activeTab?.isExecuting && "Executing..."}
        </div>
      </div>

      {/* Tabs */}
      <div className="flex items-center border-b bg-muted/20 overflow-x-auto customScrollbar scrollbar-gutter-auto">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              "flex items-center gap-1 px-3 py-1.5 cursor-pointer border-r text-sm whitespace-nowrap shrink-0",
              "hover:bg-muted/50 transition-colors",
              activeTabId === tab.id
                ? "bg-background border-b-2 border-b-primary"
                : "bg-muted/30",
            )}
          >
            <span className={cn(tab.isDirty && "italic")}>
              {tab.name}
              {tab.isDirty && " *"}
            </span>
            <Button
              variant="ghost"
              size="icon"
              className="h-4 w-4 p-0 hover:bg-muted"
              onClick={(e) => handleCloseTab(e, tab.id)}
            >
              <X className="h-3 w-3" />
            </Button>
          </div>
        ))}
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 mx-1"
          onClick={handleNewTab}
        >
          <Plus className="h-4 w-4" />
        </Button>
      </div>

      <div className="flex-1 overflow-hidden">
        {activeTab ? (
          <BaseMonacoEditor
            value={activeTab.content}
            onChange={handleContentChange}
            onMount={handleEditorMount}
            language="sql"
            options={{
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
            }}
          />
        ) : (
          <div className="h-full flex flex-col items-center justify-center text-muted-foreground">
            <Code className="h-12 w-12 mb-4 opacity-30" />
            <p className="text-sm">No query open</p>
            <Button variant="link" size="sm" onClick={handleNewTab}>
              Create new query
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
