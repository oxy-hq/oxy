import useThread from "@/hooks/api/useThread";
import { Bot } from "lucide-react";
import { useParams } from "react-router-dom";
import { Separator } from "@/components/ui/shadcn/separator";
import { useEffect, useState } from "react";
import { service } from "@/services/service";
import AnswerContent from "@/components/AnswerContent";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import PageHeader from "@/components/PageHeader";
import References from "./References";
import { Reference } from "@/types/chat";
import Steps from "./Steps";

const STEP_MAP = {
  execute_sql: "Execute SQL",
  visualize: "Generate visualization",
  retrieve: "Retrieve data",
};

const Thread = () => {
  const { threadId } = useParams();
  const { data: thread, isSuccess } = useThread(threadId ?? "");
  const [answerStream, setAnswerStream] = useState<string | null>(null);
  const [references, setReferences] = useState<Reference[]>(
    thread?.references || [],
  );

  const [steps, setSteps] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const queryClient = useQueryClient();

  useEffect(() => {
    if (isSuccess) {
      if (!thread?.answer) {
        setIsLoading(true);
        // eslint-disable-next-line promise/catch-or-return
        service
          .ask(threadId ?? "", (answer) => {
            if (answer.step) {
              setSteps((pre) => {
                if (
                  Object.keys(STEP_MAP).includes(answer.step) &&
                  pre.at(-1) !== answer.step
                ) {
                  return [...pre, answer.step];
                }
                return pre;
              });
            }
            setAnswerStream((pre) =>
              pre ? pre + answer.content : answer.content,
            );
            setReferences((pre) => {
              if (answer.references) {
                return [...pre, ...answer.references];
              }
              return pre;
            });
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

  useEffect(() => {
    if (isSuccess) {
      if (thread?.references) {
        setReferences(thread.references);
      }
    }
  }, [isSuccess, thread]);

  const answer = thread?.answer ? thread?.answer : answerStream;

  const showAnswer = answer || steps.length > 0;

  const showAgentThinking = isLoading && !showAnswer;

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

          {showAgentThinking && (
            <div className="flex gap-1">
              <img className="w-8 h-8" src="/oxy-loading.gif" />
              <p className="text-muted-foreground">Agent is thinking...</p>
            </div>
          )}
          {showAnswer && (
            <div className="p-6 rounded-xl bg-base-card border border-base-border shadow-sm flex flex-col gap-2 ">
              <div className="flex gap-1 items-center h-12 justify-start">
                <img className="w-[24px] h-[24px]" src="/logo.svg" alt="Oxy" />
                <p className="text-xl text-card-foreground font-semibold">
                  Answer
                </p>
              </div>
              <Steps steps={steps} isLoading={isLoading} />

              <AnswerContent content={answer || ""} />
            </div>
          )}
          <div className="mt-2 flex">
            {references.length > 0 && <References references={references} />}
          </div>
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
