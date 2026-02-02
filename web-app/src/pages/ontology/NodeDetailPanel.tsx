import { useEffect, useState } from "react";
import { X, Pencil, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { FileService } from "@/services/api/files";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useNavigate } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";
import { OntologyNode } from "@/types/ontology";

interface NodeDetailPanelProps {
  node: OntologyNode | null;
  onClose: () => void;
}

export function NodeDetailPanel({ node, onClose }: NodeDetailPanelProps) {
  const [content, setContent] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const { project, branchName } = useCurrentProjectBranch();
  const navigate = useNavigate();

  const typeLabels: Record<string, string> = {
    agent: "Agent",
    workflow: "Automation",
    topic: "Topic",
    view: "View",
    sql_query: "SQL Query",
    table: "Table",
    entity: "Entity",
  };

  useEffect(() => {
    if (!node) {
      setContent(null);
      return;
    }

    const loadContent = async () => {
      // Only load content for nodes that have file paths
      const filePath = node.data.path;
      if (!filePath) {
        setContent(null);
        return;
      }

      setIsLoading(true);
      try {
        const fileContent = await FileService.getFile(
          project.id,
          btoa(filePath),
          branchName,
        );
        setContent(fileContent);
      } catch (error) {
        console.error("Failed to load file content:", error);
        setContent(null);
      } finally {
        setIsLoading(false);
      }
    };

    loadContent();
  }, [node, project.id, branchName]);

  if (!node) return null;

  const handleOpenInIDE = () => {
    const filePath = node.data.path;
    if (filePath) {
      const ideUri = ROUTES.PROJECT(project.id).IDE.FILES.FILE(btoa(filePath));
      navigate(ideUri);
    }
  };

  const isFileNode =
    node.type === "workflow" ||
    node.type === "view" ||
    node.type === "topic" ||
    node.type === "agent" ||
    node.type === "sql_query";

  return (
    <div
      className="fixed right-0 top-0 h-full w-96 bg-background border-l border-border shadow-xl z-50 flex flex-col"
      style={{ animation: "slideIn 0.2s ease-out" }}
    >
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex-1 min-w-0">
          <h2 className="text-lg font-semibold truncate">{node.label}</h2>
          <p className="text-sm text-muted-foreground">
            {typeLabels[node.type] || node.type}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {isFileNode && node.data.path && (
            <Button
              variant="ghost"
              size="icon"
              onClick={handleOpenInIDE}
              title="Open in IDE"
            >
              <Pencil className="w-4 h-4" />
            </Button>
          )}
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="w-4 h-4" />
          </Button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4">
        {/* Metadata */}
        <div className="space-y-3 mb-4">
          {node.data.path && (
            <div>
              <p className="text-xs font-medium text-muted-foreground mb-1">
                Path
              </p>
              <p className="text-sm font-mono bg-muted px-2 py-1 rounded text-xs break-all">
                {node.data.path}
              </p>
            </div>
          )}
          {node.data.description && (
            <div>
              <p className="text-xs font-medium text-muted-foreground mb-1">
                Description
              </p>
              <p className="text-sm">{node.data.description}</p>
            </div>
          )}
          {node.data.database && (
            <div>
              <p className="text-xs font-medium text-muted-foreground mb-1">
                Database
              </p>
              <p className="text-sm font-mono">{node.data.database}</p>
            </div>
          )}
          {node.data.datasource && (
            <div>
              <p className="text-xs font-medium text-muted-foreground mb-1">
                Datasource
              </p>
              <p className="text-sm font-mono">{node.data.datasource}</p>
            </div>
          )}
        </div>

        {/* File Content */}
        {isFileNode && (
          <div>
            <div className="flex items-center justify-between mb-2">
              <p className="text-xs font-medium text-muted-foreground">
                {node.type === "sql_query" ? "SQL Query" : "File Contents"}
              </p>
            </div>
            {isLoading && (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
              </div>
            )}
            {!isLoading && content && (
              <pre className="text-xs bg-muted p-3 rounded overflow-auto max-h-96 whitespace-pre-wrap break-words">
                {content}
              </pre>
            )}
            {!isLoading && !content && (
              <p className="text-sm text-muted-foreground italic">
                No content available
              </p>
            )}
          </div>
        )}
      </div>

      <style>{`
        @keyframes slideIn {
          from {
            transform: translateX(100%);
          }
          to {
            transform: translateX(0);
          }
        }
      `}</style>
    </div>
  );
}
