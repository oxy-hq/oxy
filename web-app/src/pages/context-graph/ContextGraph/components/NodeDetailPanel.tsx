import { Pencil } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useFile from "@/hooks/api/files/useFile";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import useCurrentProject from "@/stores/useCurrentProject";
import type { ContextGraphNode } from "@/types/contextGraph";
import { TYPE_LABEL_SINGULAR } from "../constants";

interface NodeDetailPanelProps {
  node: ContextGraphNode | null;
  onClose: () => void;
}

const FILE_NODE_TYPES = new Set(["workflow", "procedure", "view", "topic", "agent", "sql_query"]);

function getLanguage(node: ContextGraphNode) {
  if (node.type === "sql_query") return "sql";
  const ext = node.data.path?.split(".").pop();
  if (ext === "sql") return "sql";
  return "yaml";
}

export function NodeDetailPanel({ node, onClose }: NodeDetailPanelProps) {
  const { project } = useCurrentProject();
  const navigate = useNavigate();

  const pathb64 = node?.data.path ? encodeBase64(node.data.path) : "";
  const isFileNode = node ? FILE_NODE_TYPES.has(node.type) : false;
  const { data: content, isLoading, error } = useFile(pathb64, isFileNode && !!pathb64);

  if (!node) return null;

  const handleOpenInIDE = () => {
    const filePath = node.data.path;
    if (filePath && project) {
      navigate(ROUTES.PROJECT(project.id).IDE.FILES.FILE(encodeBase64(filePath)));
    }
  };

  return (
    <Panel className='fixed top-0 right-0 z-50 w-96 border-l shadow-xl' animate>
      <PanelHeader
        title={node.label}
        subtitle={TYPE_LABEL_SINGULAR[node.type] || node.type}
        onClose={onClose}
        actions={
          isFileNode && node.data.path ? (
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              onClick={handleOpenInIDE}
              title='Open in IDE'
            >
              <Pencil className='h-4 w-4' />
            </Button>
          ) : undefined
        }
      />
      <PanelContent>
        <div className='mb-4 space-y-3'>
          {node.data.path && (
            <div>
              <p className='mb-1 font-medium text-muted-foreground text-xs'>Path</p>
              <p className='break-all rounded bg-muted px-2 py-1 font-mono text-xs'>
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

        {isFileNode && (
          <div>
            <p className='mb-2 font-medium text-muted-foreground text-xs'>
              {node.type === "sql_query" ? "SQL Query" : "File Contents"}
            </p>
            {isLoading && (
              <div className='flex items-center justify-center py-8'>
                <Spinner className='size-6 text-muted-foreground' />
              </div>
            )}
            {!isLoading && content && (
              <SyntaxHighlighter
                language={getLanguage(node)}
                style={oneDark}
                PreTag='div'
                className='rounded-lg! text-xs!'
                lineProps={{ style: { wordBreak: "break-all", whiteSpace: "pre-wrap" } }}
                wrapLines
              >
                {content}
              </SyntaxHighlighter>
            )}
            {!isLoading && error && <ErrorAlert message='Failed to load file content' />}
            {!isLoading && !content && !error && (
              <p className='text-muted-foreground text-sm italic'>No content available</p>
            )}
          </div>
        )}
      </PanelContent>
    </Panel>
  );
}
