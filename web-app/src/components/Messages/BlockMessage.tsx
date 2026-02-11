import { Fullscreen, LinkIcon, LoaderCircle } from "lucide-react";
import { encodeBase64 } from "@/libs/encoding";
import type { Block, RunInfo } from "@/services/types";
import {
  useMessageContent,
  useMessageStreaming,
  useSelectedMessageReasoning
} from "@/stores/agentic";
import type { Display, TableDisplay } from "@/types/app";
import type { Message } from "@/types/chat";
import AppPreview from "../AppPreview";
import { DisplayBlock } from "../AppPreview/Displays";
import Markdown from "../Markdown";
import TableVirtualized from "../Markdown/components/TableVirtualized";
import { Button } from "../ui/shadcn/button";
import MessageHeader from "./MessageHeader";

interface BlockMessageProps {
  message: Message;
  showAvatar?: boolean;
  prompt?: string;
  toggleReasoning?: (runInfo: RunInfo) => void;
}

const BlockMessage = ({ message, showAvatar, toggleReasoning }: BlockMessageProps) => {
  const { run_info: runInfo } = message;
  const { selectBlock } = useSelectedMessageReasoning();
  const content = useMessageContent(runInfo);
  const isStreaming = useMessageStreaming(runInfo);
  const error = runInfo?.error || (runInfo?.status === "canceled" && "Agent run was cancelled");

  if (!runInfo) {
    return null;
  }

  return (
    <div className='mb-4 flex w-full flex-col gap-2'>
      <MessageHeader
        isHuman={false}
        createdAt={message.created_at}
        tokensUsage={{
          inputTokens: message.usage.inputTokens,
          outputTokens: message.usage.outputTokens
        }}
      />

      <div className='flex w-full items-start gap-2'>
        {showAvatar && <img className='h-8 w-8 rounded-full' src='/logo.svg' alt='Oxy' />}
        <div className='w-full flex-1'>
          <div className='flex w-full flex-col gap-2 overflow-x-auto rounded-xl border border-base-border bg-base-card p-4 shadow-sm'>
            <div>
              <Button
                // className="flex gap-2 items-start"
                variant={"outline"}
                onClick={() => toggleReasoning?.(runInfo)}
              >
                {isStreaming ? (
                  <>
                    <LoaderCircle className='h-2 w-2 animate-spin text-muted-foreground' />
                    <div className='text-muted-foreground'>Agent is thinking...</div>
                  </>
                ) : (
                  <>
                    <LinkIcon />
                    <p className='text-muted-foreground'>Show reasoning</p>
                  </>
                )}
              </Button>
            </div>

            {error ? (
              <span className='text-red-800'>{error}</span>
            ) : (
              !!content &&
              content?.map((block) => (
                <BlockContent
                  key={block.id}
                  block={block}
                  onFullscreen={(blockId) => selectBlock(blockId, runInfo)}
                />
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export const BlockContent = ({
  block,
  onFullscreen
}: {
  block: Block;
  onFullscreen?: (blockId: string) => void;
}) => {
  return (
    <div className='relative'>
      <BlockComponent block={block} />
      {!!onFullscreen && isFullscreenableBlock(block) && (
        <Button
          variant='ghost'
          size='icon'
          className='absolute top-2 right-2 opacity-50 hover:opacity-100'
          onClick={() => onFullscreen(block.id)}
        >
          <Fullscreen size={16} />
        </Button>
      )}
    </div>
  );
};

const isFullscreenableBlock = (block: Block) => {
  return ["sql", "viz", "data_app"].includes(block.type);
};

const BlockComponent = ({ block }: { block: Block }) => {
  switch (block.type) {
    case "text":
      return <Markdown>{block.content}</Markdown>;
    case "sql":
      return (
        <>
          <span className='text-bold text-sm'>SQL Query</span>
          <Markdown>{`\`\`\`sql\n${block.sql_query}\n\`\`\``}</Markdown>
          <span className='text-bold text-sm'>Results</span>
          <TableVirtualized table_id='0' tables={[block.result]} />
        </>
      );
    case "viz":
      return (
        <>
          <DisplayBlock
            display={block.config as Display}
            data={{
              [(block.config as TableDisplay).data]: {
                file_path: (block.config as TableDisplay).data
              }
            }}
          />
          {/* <pre className="mt-2 text-sm text-muted-foreground">
            {JSON.stringify(block.config)}
          </pre> */}
        </>
      );
    case "data_app":
      return (
        <div className='relative h-96'>
          <AppPreview appPath64={encodeBase64(block.file_path)} />
        </div>
      );
    default:
      return <div>Unsupported block type: {block.type}</div>;
  }
};

export default BlockMessage;
