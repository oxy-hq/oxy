import { Message } from "@/types/chat";
import MessageHeader from "./MessageHeader";
import { LinkIcon, LoaderCircle } from "lucide-react";
import { Button } from "../ui/shadcn/button";
import { useMessageContent, useMessageStreaming } from "@/stores/agentic";
import { RunInfo } from "@/services/types";

interface BlockMessageProps {
  message: Message;
  showAvatar?: boolean;
  prompt?: string;
  toggleReasoning?: (runInfo: RunInfo) => void;
}

const BlockMessage = ({
  message,
  showAvatar,
  toggleReasoning,
}: BlockMessageProps) => {
  const { run_info: runInfo } = message;
  const content = useMessageContent(runInfo);
  const isStreaming = useMessageStreaming(runInfo);
  const error =
    runInfo?.error ||
    (runInfo?.status == "canceled" && "Agent run was cancelled");

  if (!runInfo) {
    return null;
  }

  return (
    <div className="flex flex-col gap-2 w-full mb-4">
      <MessageHeader
        isHuman={false}
        createdAt={message.created_at}
        tokensUsage={{
          inputTokens: message.usage.inputTokens,
          outputTokens: message.usage.outputTokens,
        }}
      />

      <div className="flex gap-2 items-start w-full">
        {showAvatar && (
          <img className="w-8 h-8 rounded-full" src="/logo.svg" alt="Oxy" />
        )}
        <div className="flex-1 w-full">
          <div className="p-4 w-full rounded-xl bg-base-card border border-base-border shadow-sm flex flex-col gap-2 overflow-x-auto">
            {!error ? (
              <>
                <div>
                  <Button
                    // className="flex gap-2 items-start"
                    variant={"outline"}
                    onClick={() => toggleReasoning?.(runInfo)}
                  >
                    {isStreaming ? (
                      <>
                        <LoaderCircle className="w-2 h-2 animate-spin text-muted-foreground" />
                        <div className="text-muted-foreground">
                          Agent is thinking...
                        </div>
                      </>
                    ) : (
                      <>
                        <LinkIcon />
                        <p className="text-muted-foreground">Show reasoning</p>
                      </>
                    )}
                  </Button>
                </div>
                {content}
              </>
            ) : (
              <span className="text-red-800">{error}</span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default BlockMessage;
