import {
  Dialog,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Code } from "lucide-react";
import { SqlQueryReference } from "@/types/chat";
import CodeBlock from "@/components/CodeBlock";
import { ReferenceItemContainer } from "./ReferenceItemContainer";
import { QueryResultTable } from "./QueryResultTable";

export type QueryReferenceProps = {
  reference: SqlQueryReference;
};

export const QueryReference = ({ reference }: QueryReferenceProps) => {
  const metadata = reference;
  return (
    <>
      <Dialog>
        <DialogTrigger>
          <ReferenceItemContainer>
            <div className="p-4 gap-4 w-50 flex flex-col items-center justify-center overflow-hidden text-muted-foreground">
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
        <DialogContent className="[&>button]:hidden p-2 break-all">
          <div className="text-sm">
            <div className="flex items-center gap-2 justify-start w-full">
              <Code size={16} />
              <span className="truncate">Query</span>
            </div>
            <CodeBlock className="language-sql">{metadata.sql_query}</CodeBlock>
            <QueryResultTable
              result={metadata.result}
              isTruncated={metadata.is_result_truncated}
            />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
};
