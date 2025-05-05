import useThread from "@/hooks/api/useThread";
import { Bot } from "lucide-react";
import { useParams } from "react-router-dom";
import { Separator } from "@/components/ui/shadcn/separator";
import { useEffect, useState } from "react";
import { service } from "@/services/service";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import PageHeader from "@/components/PageHeader";
import { Message } from "@/types/chat";
import AgentMessage from "@/components/AgentMessage";

const STEP_MAP = {
  execute_sql: "Execute SQL",
  visualize: "Generate visualization",
  retrieve: "Retrieve data",
};

const Thread = () => {
  const { threadId } = useParams();
  const { data: thread, isSuccess } = useThread(threadId ?? "");
  const [message, setMessage] = useState<Message>({
    content: "",
    references: [],
    steps: [],
    isUser: false,
    isStreaming: false,
  });
  const queryClient = useQueryClient();

  useEffect(() => {
    if (isSuccess) {
      if (!thread?.answer) {
        setMessage((pre) => ({
          ...pre,
          content: "",
          references: [],
          steps: [],
          isStreaming: true,
        }));
        // eslint-disable-next-line promise/catch-or-return
        service
          .ask(threadId ?? "", (answer) => {
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
      } else {
        setMessage((prev) => ({
          ...prev,
          content: thread.answer,
          references: thread.references,
          steps: [],
          isUser: false,
          isStreaming: false,
        }));
      }
    }
  }, [isSuccess, thread, threadId, queryClient]);

  return (
    <div className="flex flex-col h-full">
      <PageHeader className="border-b-1 border-border items-center">
        <div className="p-2 flex items-center justify-center flex-1 h-full">
          <div className="flex gap-1 items-center text-muted-foreground">
            <Bot className="w-4 h-4 min-w-4 min-h-4" />
            <p className="text-sm break-all">{thread?.agent}</p>
          </div>
          <div className="px-4 h-full flex items-stretch">
            <Separator orientation="vertical" />
          </div>

          <p className="text-sm text-base-foreground">{thread?.title}</p>
        </div>
      </PageHeader>

      <div className="overflow-y-auto customScrollbar">
        <div className="flex-1 max-w-[742px] px-4 mx-auto pb-4">
          <div className="pt-8 pb-6 text-3xl font-semibold text-base-foreground">
            {thread?.question}
          </div>

          <AgentMessage message={message} />
        </div>
      </div>
    </div>
  );
};

const ThreadPage = () => {
  const { threadId } = useParams();
  return <Thread key={threadId} />;
};

export default ThreadPage;
