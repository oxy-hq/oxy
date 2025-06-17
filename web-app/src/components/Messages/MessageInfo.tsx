import { InfoIcon } from "lucide-react";
import dayjs from "dayjs";

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { FunctionComponent } from "react";

interface MessageInfoProps {
  createdAt?: string;
  tokensUsage?: {
    inputTokens: number;
    outputTokens: number;
  };
}

const MessageInfo: FunctionComponent<MessageInfoProps> = ({
  createdAt,
  tokensUsage,
}) => {
  return (
    <span className="text-xs text-muted-foreground ml-auto">
      {createdAt ? dayjs(createdAt).fromNow() : null}
      {tokensUsage &&
      (tokensUsage.inputTokens !== 0 || tokensUsage.outputTokens !== 0) ? (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger className="inline ml-1" asChild>
              <InfoIcon className="w-3 h-3" />
            </TooltipTrigger>
            <TooltipContent arrowClassName="invisible" className="bg-muted">
              <div className="flex flex-col space-y-1">
                <div className="font-medium text-sm">
                  {tokensUsage.inputTokens + tokensUsage.outputTokens} tokens
                  used
                </div>
                <div className="border-t pt-1 space-y-0.5">
                  <div className="flex justify-between gap-4">
                    <span className="text-muted-foreground">Input:</span>
                    <span className="font-medium">
                      {tokensUsage.inputTokens}
                    </span>
                  </div>
                  <div className="flex justify-between gap-4">
                    <span className="text-muted-foreground">Output:</span>
                    <span className="font-medium">
                      {tokensUsage.outputTokens}
                    </span>
                  </div>
                </div>
              </div>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      ) : null}
    </span>
  );
};

export default MessageInfo;
