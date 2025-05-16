import {
  Dialog,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Code, X } from "lucide-react";
import { SqlQueryReference } from "@/types/chat";
import CodeBlock from "@/components/CodeBlock";
import { ReferenceItemContainer } from "./ReferenceItemContainer";
import { QueryResultTable } from "./QueryResultTable";
import { Button } from "@/components/ui/shadcn/button";
import { useState } from "react";

export type QueryReferenceProps = {
  reference: SqlQueryReference;
};

export const QueryReference = ({ reference }: QueryReferenceProps) => {
  const metadata = reference;
  const [isOpen, setIsOpen] = useState(false);
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
              <Button variant="ghost" onClick={() => setIsOpen(false)}>
                <X />
              </Button>
            </div>
            <div className="p-4 pt-0 flex flex-col gap-4">
              <div className="max-h-80 overflow-auto customScrollbar">
                <CodeBlock className="language-sql !m-0">
                  {metadata.sql_query}
                </CodeBlock>
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
