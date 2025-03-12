import useThread from "@/hooks/api/useThread";
import { Bot } from "lucide-react";
import { useParams } from "react-router-dom";
import { Separator } from "@/components/ui/shadcn/separator";
import { useEffect, useState } from "react";
import { service } from "@/services/service";
import AnswerContent from "@/components/AnswerContent";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";

const Thread = () => {
  const { threadId } = useParams();
  const { data: thread, isSuccess } = useThread(threadId ?? "");
  const [answerStream, setAnswerStream] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const queryClient = useQueryClient();

  useEffect(() => {
    if (isSuccess) {
      if (!thread?.answer) {
        setIsLoading(true);
        // eslint-disable-next-line promise/catch-or-return
        service
          .ask(threadId ?? "", (answer) => {
            console.log(answer);
            setAnswerStream((pre) =>
              pre ? pre + answer.content : answer.content,
            );
            setIsLoading(false);
          })
          .finally(() => {
            setIsLoading(false);
            queryClient.invalidateQueries({
              queryKey: queryKeys.thread.all,
            });
          });
      }
    }
  }, [isSuccess, thread, threadId, queryClient]);

  const answer = thread?.answer ? thread?.answer : answerStream;

  return (
    <div className="flex flex-col h-full">
      <div className="border-b-1 border-border p-4 flex items-center justify-center">
        <div className="flex gap-1 items-center text-muted-foreground">
          <Bot className="w-4 h-4" />
          <p className="text-sm">{thread?.agent}</p>
        </div>
        <div className="px-4 h-full flex items-stretch">
          <Separator orientation="vertical" />
        </div>

        <p className="text-sm text-base-foreground">{thread?.title}</p>
      </div>

      <div className="overflow-y-auto customScrollbar">
        <div className="flex-1 max-w-[742px] px-4 mx-auto pb-4">
          <div className="pt-8 pb-6 text-3xl font-bold text-base-foreground">
            {thread?.question}
          </div>

          {isLoading && (
            <div className="flex gap-1 ju">
              <img className="w-8 h-8" src="/onyx-loading.gif" />
              <p className="text-muted-foreground">Agent is thinking...</p>
            </div>
          )}
          {!isLoading && answer && (
            <div className="p-6 rounded-xl bg-base-card border border-base-border shadow-sm flex flex-col gap-2 ">
              <div className="flex gap-1 items-center h-12 justify-start">
                <img className="w-[24px] h-[24px]" src="/logo.svg" alt="Onyx" />
                <p className="text-xl text-card-foreground font-semibold">
                  Answer
                </p>
              </div>

              <AnswerContent content={answer} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default Thread;
