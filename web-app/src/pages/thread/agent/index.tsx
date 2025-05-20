import AgentMessage from "@/components/AgentMessage";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import queryKeys from "@/hooks/api/queryKey";
import { service } from "@/services/service";
import { STEP_MAP } from "@/types/agent";
import { Message, ThreadItem } from "@/types/chat";
import { useQueryClient } from "@tanstack/react-query";
import { Bot } from "lucide-react";
import { useRef } from "react";
import { useState } from "react";
import { useEffect } from "react";

const AgentThread = ({ thread }: { thread: ThreadItem }) => {
  const queryClient = useQueryClient();
  const [message, setMessage] = useState<Message>({
    content: "",
    references: [],
    steps: [],
    isUser: false,
    isStreaming: false,
  });

  const hasRun = useRef(false);

  useEffect(() => {
    if (hasRun.current) {
      return;
    }

    hasRun.current = true;

    if (thread.output) {
      setMessage((prev) => ({
        ...prev,
        content: thread.output,
        references: thread.references,
        steps: [],
        isUser: false,
        isStreaming: false,
      }));
      return;
    }

    setMessage((pre) => ({
      ...pre,
      content: "",
      references: [],
      steps: [],
      isStreaming: true,
    }));
    // eslint-disable-next-line promise/catch-or-return
    service
      .ask(thread.id, (answer) => {
        setMessage((prevMessage) => {
          const { content, references, steps } = prevMessage;
          const shouldAddStep =
            answer.step &&
            Object.keys(STEP_MAP).includes(answer.step) &&
            steps.at(-1) !== answer.step;

          return {
            content: content + answer.content,
            references: answer.references
              ? [...references, ...answer.references]
              : references,
            steps: shouldAddStep ? [...steps, answer.step] : steps,
            isUser: false,
            isStreaming: true,
          };
        });
      })
      .finally(() => {
        setMessage((prev) => {
          return { ...prev, isStreaming: false };
        });
        queryClient.invalidateQueries({
          queryKey: queryKeys.thread.all,
        });
      });
  }, [queryClient, thread]);

  return (
    <div className="flex flex-col h-full">
      <PageHeader className="border-b-1 border-border items-center">
        <div className="p-2 flex items-center justify-center flex-1 h-full">
          <div className="flex gap-1 items-center text-muted-foreground">
            <Bot className="w-4 h-4 min-w-4 min-h-4" />
            <p className="text-sm break-all">{thread?.source}</p>
          </div>
          <div className="px-4 h-full flex items-stretch">
            <Separator orientation="vertical" />
          </div>

          <p className="text-sm text-base-foreground">{thread?.title}</p>
        </div>
      </PageHeader>

      <div className="overflow-y-auto customScrollbar">
        <div className="flex-1 max-w-[742px] px-4 mx-auto pb-4">
          {thread && (
            <>
              <div className="pt-8 pb-6 text-3xl font-semibold text-base-foreground">
                {thread?.input}
              </div>
              <AgentMessage message={message} prompt={thread.input} />
            </>
          )}
        </div>
      </div>
    </div>
  );
};

export default AgentThread;
