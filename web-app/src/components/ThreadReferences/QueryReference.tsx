import { Code, Download, Loader2, Save, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import CodeBlock from "@/components/Markdown/components/CodeBlock";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogTrigger } from "@/components/ui/shadcn/dialog";
import useCreateWorkflowFromQueryMutation from "@/hooks/api/workflows/useCreateWorkflowFromQueryMutation";
import type { SqlQueryReference } from "@/types/chat";
import { QueryResultTable } from "./QueryResultTable";
import { ReferenceItemContainer } from "./ReferenceItemContainer";

export type QueryReferenceProps = {
  reference: SqlQueryReference;
  prompt?: string;
};

export const QueryReference = ({ reference, prompt }: QueryReferenceProps) => {
  const metadata = reference;
  const [isOpen, setIsOpen] = useState(false);
  const { mutate: createWorkflowFromQuery, isPending } = useCreateWorkflowFromQueryMutation(() => {
    toast.success(`Workflow created successfully`);
  });

  const handleSaveToWorkflow = () => {
    if (prompt) {
      createWorkflowFromQuery({
        prompt: prompt,
        query: metadata.sql_query,
        database: metadata.database
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
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger className='h-21'>
        <ReferenceItemContainer isOpen={isOpen}>
          <div className='flex w-50 flex-col items-center justify-center gap-2 overflow-hidden px-4 py-2 text-muted-foreground'>
            <div className='flex w-full items-center justify-start gap-2 text-sm'>
              <Code size={16} />
              <span className='truncate'>QUERY</span>
            </div>
            <span className='line-clamp-2 w-full text-start font-mono text-sm leading-[20px]'>
              {metadata.sql_query}
            </span>
          </div>
        </ReferenceItemContainer>
      </DialogTrigger>
      <DialogContent showOverlay={false} className='max-w-[50vw]! break-all p-0 [&>button]:hidden'>
        <div className='max-w-[50vw] text-sm'>
          <div className='flex items-center justify-between py-2 pr-2 pl-4'>
            <div className='flex w-full items-center justify-start gap-1'>
              <div className='flex items-center justify-center p-2'>
                <Code size={16} />
              </div>
              <span className='truncate'>Query</span>
            </div>
            <div className='flex gap-2'>
              {prompt && (
                <Button
                  disabled={isPending}
                  variant='secondary'
                  onClick={handleSaveToWorkflow}
                  title='Save to Workflow'
                >
                  {isPending ? (
                    <Loader2 className='animate-spin' size={16} color='currentColor' />
                  ) : (
                    <Save />
                  )}
                </Button>
              )}
              <Button variant='ghost' onClick={() => setIsOpen(false)}>
                <X />
              </Button>
            </div>
          </div>
          <div className='flex flex-col gap-4 p-4 pt-0'>
            <div className='customScrollbar relative max-h-80 overflow-auto'>
              <CodeBlock className='language-sql !m-0 pr-[54px]!'>{metadata.sql_query}</CodeBlock>
              <Button
                title='Download SQL'
                className='absolute top-2 right-2'
                variant='outline'
                size='icon'
                onClick={handleDownloadSql}
              >
                <Download className='h-4 w-4' />
              </Button>
            </div>
            <QueryResultTable
              result={metadata.result}
              resultFile={metadata.result_file}
              isTruncated={metadata.is_result_truncated}
            />
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
};
