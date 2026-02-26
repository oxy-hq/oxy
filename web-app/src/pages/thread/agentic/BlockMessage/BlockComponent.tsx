import AppPreview from "@/components/AppPreview";
import { DisplayBlock } from "@/components/AppPreview/Displays";
import Markdown from "@/components/Markdown";
import TableVirtualized from "@/components/Markdown/components/TableVirtualized";
import { encodeBase64 } from "@/libs/encoding";
import type { Block } from "@/services/types";
import type { Display, TableDisplay } from "@/types/app";

const FULLSCREENABLE_BLOCK_TYPES = new Set(["sql", "semantic_query", "viz", "data_app"]);

export const isFullscreenableBlock = (block: Block) => FULLSCREENABLE_BLOCK_TYPES.has(block.type);

const TextBlock = ({ content }: { content: string }) => <Markdown>{content}</Markdown>;

const SemanticQueryBlock = ({ sqlQuery, results }: { sqlQuery?: string; results?: string[][] }) => (
  <>
    {sqlQuery && (
      <>
        <span className='text-bold text-sm'>Generated SQL</span>
        <Markdown>{`\`\`\`sql\n${sqlQuery}\n\`\`\``}</Markdown>
      </>
    )}
    {results && results.length > 0 && (
      <>
        <span className='text-bold text-sm'>Results</span>
        <TableVirtualized table_id='0' tables={[results]} />
      </>
    )}
  </>
);

const SqlBlock = ({ sqlQuery, result }: { sqlQuery: string; result: string[][] }) => (
  <>
    <span className='text-bold text-sm'>SQL Query</span>
    <Markdown>{`\`\`\`sql\n${sqlQuery}\n\`\`\``}</Markdown>
    <span className='text-bold text-sm'>Results</span>
    <TableVirtualized table_id='0' tables={[result]} />
  </>
);

const VizBlock = ({ config }: { config: Display }) => (
  <DisplayBlock
    display={config}
    data={{
      [(config as TableDisplay).data]: {
        file_path: (config as TableDisplay).data
      }
    }}
  />
);

const DataAppBlock = ({ filePath }: { filePath: string }) => (
  <div className='relative h-96 space-y-1.5 rounded-lg border border-border bg-card p-3'>
    <AppPreview appPath64={encodeBase64(filePath)} />
  </div>
);

const BlockComponent = ({ block }: { block: Block }) => {
  switch (block.type) {
    case "text":
      return <TextBlock content={block.content} />;
    case "semantic_query":
      return <SemanticQueryBlock sqlQuery={block.sql_query} results={block.results} />;
    case "sql":
      return <SqlBlock sqlQuery={block.sql_query} result={block.result} />;
    case "viz":
      return <VizBlock config={block.config as Display} />;
    case "data_app":
      return <DataAppBlock filePath={block.file_path} />;
    default:
      return <div>Unsupported block type: {block.type}</div>;
  }
};

export default BlockComponent;
