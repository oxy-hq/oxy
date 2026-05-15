/**
 * SQL query reference card — displays an agent-executed query and its
 * result inline in chat, with a click-to-expand dialog for the full SQL
 * + result table.
 *
 * The "Save to Workflow" path that this component used to expose was
 * dropped along with the legacy `/workflows/from-query` endpoint. Use
 * the workflow editor in the IDE to author new workflows.
 */

import { Code, Download, X } from "lucide-react";
import { useState } from "react";

import CodeBlock from "@/components/Markdown/components/CodeBlock";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogTrigger } from "@/components/ui/shadcn/dialog";
import type { SqlQueryReference } from "@/types/chat";
import { QueryResultTable } from "./QueryResultTable";
import { ReferenceItemContainer } from "./ReferenceItemContainer";

export type QueryReferenceProps = {
  reference: SqlQueryReference;
};

export const QueryReference = ({ reference }: QueryReferenceProps) => {
  const metadata = reference;
  const [isOpen, setIsOpen] = useState(false);

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
            <Button variant='ghost' onClick={() => setIsOpen(false)}>
              <X />
            </Button>
          </div>
          <div className='flex flex-col gap-4 p-4 pt-0'>
            <div className='relative max-h-80 overflow-auto'>
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
