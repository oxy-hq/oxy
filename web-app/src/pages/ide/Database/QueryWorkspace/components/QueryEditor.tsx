import { get } from "lodash";
import { Code, Loader2, Play, Plus, Save, X } from "lucide-react";
import type { editor } from "monaco-editor";
import { useCallback, useRef, useState } from "react";
import { toast } from "sonner";
import { format as formatSQL } from "sql-formatter";
import { BaseMonacoEditor, useMonacoSetup } from "@/components/MonacoEditor";
import DatabaseSelector from "@/components/sql/DatabaseSelector";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import { DatabaseService } from "@/services/api";
import useDatabaseClient from "@/stores/useDatabaseClient";

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
    setTabError
  } = useDatabaseClient();

  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const [tabToClose, setTabToClose] = useState<{ id: string; name: string } | null>(null);

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
        activeTab.selectedDatabase
      );

      const executionTime = performance.now() - startTime;

      // Handle parquet response (file reference)
      if (response && typeof response === "object" && "file_name" in response) {
        toast.success(`Query executed in ${executionTime.toFixed(0)}ms (results saved to file)`);
        setTabResults(activeTab.id, {
          result: [],
          resultFile: (response as { file_name: string }).file_name,
          executionTime
        });
        return;
      }

      setTabResults(activeTab.id, {
        result: response as string[][],
        resultFile: undefined,
        executionTime
      });

      toast.success(`Query executed in ${executionTime.toFixed(0)}ms`);
    } catch (error) {
      const rawError =
        get(error, "response.data.error") ||
        get(error, "response.data.message") ||
        get(error, "message") ||
        "Query execution failed";

      const messageMatch = rawError.match?.(/"message":\s*"([^"]+)"/);
      const errorMessage = messageMatch ? messageMatch[1] : rawError;

      setTabError(activeTab.id, errorMessage);
      toast.error(errorMessage);
    }
  }, [activeTab, project.id, branchName, setTabExecuting, setTabResults, setTabError]);

  useMonacoSetup({ onSave, onExecute: handleRunQuery });

  const handleNewTab = () => {
    const result = addTab({ selectedDatabase: activeTab?.selectedDatabase });
    if (!result.success) {
      toast.error(result.error);
    }
  };

  const handleCloseTab = (e: React.MouseEvent, tabId: string) => {
    e.stopPropagation();
    const tab = tabs.find((t) => t.id === tabId);
    if (tab?.isDirty) {
      setTabToClose({ id: tabId, name: tab.name });
      return;
    }
    removeTab(tabId);
  };

  const handleConfirmClose = () => {
    if (tabToClose) {
      removeTab(tabToClose.id);
      setTabToClose(null);
    }
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
        logicalOperatorNewline: "before"
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
    <div className='flex h-full flex-col bg-background'>
      {/* Toolbar */}
      <div className='flex items-center justify-between border-b bg-muted/30 px-2 py-1'>
        <div className='flex items-center gap-1'>
          {/* Database Selector */}
          <div className='mr-2'>
            <DatabaseSelector
              onSelect={(db) => activeTab && updateTab(activeTab.id, { selectedDatabase: db })}
              database={activeTab?.selectedDatabase ?? null}
            />
          </div>

          <Button
            variant='ghost'
            size='sm'
            onClick={handleRunQuery}
            disabled={!activeTab || activeTab.isExecuting || !activeTab?.selectedDatabase}
            className='h-7 px-2'
          >
            {activeTab?.isExecuting ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <Play className='h-4 w-4' />
            )}
            <span className='ml-1'>Run</span>
          </Button>
          <Button
            variant='ghost'
            size='sm'
            onClick={onSave}
            disabled={!activeTab}
            className='h-7 px-2'
          >
            <Save className='h-4 w-4' />
            <span className='ml-1'>Save</span>
          </Button>
          <Button
            variant='ghost'
            size='sm'
            onClick={handleFormatSQL}
            disabled={!activeTab}
            className='h-7 px-2'
          >
            <Code className='h-4 w-4' />
            <span className='ml-1'>Format</span>
          </Button>
        </div>

        <div className='text-muted-foreground text-xs'>
          {activeTab?.isExecuting && "Executing..."}
        </div>
      </div>

      {/* Tabs */}
      <div className='customScrollbar scrollbar-gutter-auto flex items-center overflow-x-auto border-b bg-muted/20'>
        {tabs.map((tab) => (
          <div
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              "flex shrink-0 cursor-pointer items-center gap-1 whitespace-nowrap border-r px-3 py-1.5 text-sm",
              "transition-colors hover:bg-muted/50",
              activeTabId === tab.id ? "border-b-2 border-b-primary bg-background" : "bg-muted/30"
            )}
          >
            <span className={cn(tab.isDirty && "italic")}>
              {tab.name}
              {tab.isDirty && " *"}
            </span>
            <Button
              variant='ghost'
              size='icon'
              className='h-4 w-4 p-0 hover:bg-muted'
              onClick={(e) => handleCloseTab(e, tab.id)}
            >
              <X className='h-3 w-3' />
            </Button>
          </div>
        ))}
        <Button variant='ghost' size='icon' className='mx-1 h-7 w-7' onClick={handleNewTab}>
          <Plus className='h-4 w-4' />
        </Button>
      </div>

      <div className='flex-1 overflow-hidden'>
        {activeTab ? (
          <BaseMonacoEditor
            value={activeTab.content}
            onChange={handleContentChange}
            onMount={handleEditorMount}
            language='sql'
            options={{
              minimap: { enabled: false },
              scrollBeyondLastLine: true
            }}
          />
        ) : (
          <div className='flex h-full flex-col items-center justify-center text-muted-foreground'>
            <Code className='mb-4 h-12 w-12 opacity-30' />
            <p className='text-sm'>No query open</p>
            <Button variant='link' size='sm' onClick={handleNewTab}>
              Create new query
            </Button>
          </div>
        )}
      </div>

      <AlertDialog open={!!tabToClose} onOpenChange={(open) => !open && setTabToClose(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Unsaved Changes</AlertDialogTitle>
            <AlertDialogDescription>
              "{tabToClose?.name}" has unsaved changes. Are you sure you want to close it?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: "destructive" })}
              onClick={handleConfirmClose}
            >
              Close
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
