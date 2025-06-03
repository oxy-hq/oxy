import {
  Dialog,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Code, Download, Loader2, Save, X } from "lucide-react";
import { SqlQueryReference } from "@/types/chat";
import CodeBlock from "@/components/Markdown/components/CodeBlock";
import { ReferenceItemContainer } from "./ReferenceItemContainer";
import { QueryResultTable } from "./QueryResultTable";
import { Button } from "@/components/ui/shadcn/button";
import { useState } from "react";
import useCreateWorkflowFromQueryMutation from "@/hooks/api/useCreateWorkflowFromQueryMutation";
import { toast } from "sonner";

export type QueryReferenceProps = {
  reference: SqlQueryReference;
  prompt?: string;
};

export const QueryReference = ({ reference, prompt }: QueryReferenceProps) => {
  const metadata = reference;
  const [isOpen, setIsOpen] = useState(false);
  const { mutate: createWorkflowFromQuery, isPending } =
    useCreateWorkflowFromQueryMutation(() => {
      toast.success(`Workflow created successfully`);
    });

  const handleSaveToWorkflow = () => {
    if (prompt) {
      createWorkflowFromQuery({
        prompt: prompt,
        query: metadata.sql_query,
        database: metadata.database,
      });
    }
  };

  const handleDownloadSql = () => {
    const blob = new Blob([metadata.sql_query], { type: "text/plain" });
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "query.sql";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    window.URL.revokeObjectURL(url);
  };

  return (
    <>
      <Dialog open={isOpen} onOpenChange={setIsOpen}>
        <DialogTrigger className="h-21">
          <ReferenceItemContainer isOpen={isOpen}>
            <div className="px-4 py-2 gap-2 w-50 flex flex-col items-center justify-center overflow-hidden text-muted-foreground">
              <div className="flex text-sm items-center gap-2 justify-start w-full">
                <Code size={16} />
                <span className="truncate">QUERY</span>
              </div>
              <span className="w-full text-start line-clamp-2 font-mono leading-[20px] text-sm">
                {metadata.sql_query}
              </span>
            </div>
          </ReferenceItemContainer>
        </DialogTrigger>
        <DialogContent
          showOverlay={false}
          className="[&>button]:hidden break-all p-0 max-w-[50vw]!"
        >
          <div className="text-sm max-w-[50vw]">
            <div className="flex items-center justify-between pl-4 pr-2 py-2">
              <div className="flex items-center gap-1 justify-start w-full">
                <div className="p-2 flex items-center justify-center">
                  <Code size={16} />
                </div>
                <span className="truncate">Query</span>
              </div>
              <div className="flex gap-2">
                {prompt && (
                  <Button
                    disabled={isPending}
                    variant="secondary"
                    onClick={handleSaveToWorkflow}
                    title="Save to Workflow"
                  >
                    {isPending ? (
                      <Loader2
                        className="animate-spin"
                        size={16}
                        color="currentColor"
                      />
                    ) : (
                      <Save />
                    )}
                  </Button>
                )}
                <Button variant="ghost" onClick={() => setIsOpen(false)}>
                  <X />
                </Button>
              </div>
            </div>
            <div className="p-4 pt-0 flex flex-col gap-4">
              <div className="max-h-80 overflow-auto customScrollbar relative">
                <CodeBlock className="language-sql !m-0 pr-[54px]!">
                  {metadata.sql_query}
                </CodeBlock>
                <Button
                  title="Download SQL"
                  className="absolute top-2 right-2"
                  variant="outline"
                  size="icon"
                  onClick={handleDownloadSql}
                >
                  <Download className="h-4 w-4" />
                </Button>
              </div>
              <QueryResultTable
                result={metadata.result}
                isTruncated={metadata.is_result_truncated}
              />
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
};
