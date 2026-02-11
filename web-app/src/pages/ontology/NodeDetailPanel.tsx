import { Loader2, Pencil, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import { FileService } from "@/services/api/files";
import type { OntologyNode } from "@/types/ontology";

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
    entity: "Entity"
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
          encodeBase64(filePath),
          branchName
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
      const ideUri = ROUTES.PROJECT(project.id).IDE.FILES.FILE(encodeBase64(filePath));
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
      className='fixed top-0 right-0 z-50 flex h-full w-96 flex-col border-border border-l bg-background shadow-xl'
      style={{ animation: "slideIn 0.2s ease-out" }}
    >
      {/* Header */}
      <div className='flex items-center justify-between border-border border-b p-4'>
        <div className='min-w-0 flex-1'>
          <h2 className='truncate font-semibold text-lg'>{node.label}</h2>
          <p className='text-muted-foreground text-sm'>{typeLabels[node.type] || node.type}</p>
        </div>
        <div className='flex items-center gap-2'>
          {isFileNode && node.data.path && (
            <Button variant='ghost' size='icon' onClick={handleOpenInIDE} title='Open in IDE'>
              <Pencil className='h-4 w-4' />
            </Button>
          )}
          <Button variant='ghost' size='icon' onClick={onClose}>
            <X className='h-4 w-4' />
          </Button>
        </div>
      </div>

      {/* Content */}
      <div className='flex-1 overflow-auto p-4'>
        {/* Metadata */}
        <div className='mb-4 space-y-3'>
          {node.data.path && (
            <div>
              <p className='mb-1 font-medium text-muted-foreground text-xs'>Path</p>
              <p className='break-all rounded bg-muted px-2 py-1 font-mono text-sm text-xs'>
                {node.data.path}
              </p>
            </div>
          )}
          {node.data.description && (
            <div>
              <p className='mb-1 font-medium text-muted-foreground text-xs'>Description</p>
              <p className='text-sm'>{node.data.description}</p>
            </div>
          )}
          {node.data.database && (
            <div>
              <p className='mb-1 font-medium text-muted-foreground text-xs'>Database</p>
              <p className='font-mono text-sm'>{node.data.database}</p>
            </div>
          )}
          {node.data.datasource && (
            <div>
              <p className='mb-1 font-medium text-muted-foreground text-xs'>Datasource</p>
              <p className='font-mono text-sm'>{node.data.datasource}</p>
            </div>
          )}
        </div>

        {/* File Content */}
        {isFileNode && (
          <div>
            <div className='mb-2 flex items-center justify-between'>
              <p className='font-medium text-muted-foreground text-xs'>
                {node.type === "sql_query" ? "SQL Query" : "File Contents"}
              </p>
            </div>
            {isLoading && (
              <div className='flex items-center justify-center py-8'>
                <Loader2 className='h-6 w-6 animate-spin text-muted-foreground' />
              </div>
            )}
            {!isLoading && content && (
              <pre className='max-h-96 overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-3 text-xs'>
                {content}
              </pre>
            )}
            {!isLoading && !content && (
              <p className='text-muted-foreground text-sm italic'>No content available</p>
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
