import { Bot } from "lucide-react";
import { useParams } from "react-router-dom";
import { Separator } from "@/components/ui/shadcn/separator";
import { useEffect, useRef, useState } from "react";
import { service } from "@/services/service";
import AnswerContent from "@/components/AnswerContent";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import PageHeader from "@/components/PageHeader";
import ThreadSteps from "@/components/ThreadSteps";
import useTask from "@/hooks/api/useTask";
import EditorTab from "./EditorTab";
import { useSidebar } from "@/components/ui/shadcn/sidebar";

const STEP_MAP = {
  execute_sql: "Execute SQL",
  visualize: "Generate visualization",
  retrieve: "Retrieve data",
};

const Task = () => {
  const { taskId } = useParams();
  const { data: task, isSuccess } = useTask(taskId ?? "");
  const [answerStream, setAnswerStream] = useState<string | null>(null);
  const [filePath, setFilePath] = useState<string | undefined>(task?.file_path);

  const { setOpen } = useSidebar();
  const hasClosedSidebar = useRef(false);

  useEffect(() => {
    if (task?.file_path) {
      setFilePath(task.file_path);
    }
  }, [task]);

  useEffect(() => {
    if (!hasClosedSidebar.current) {
      setOpen(false);
      hasClosedSidebar.current = true;
    }
  }, [setOpen, taskId]);

  const [steps, setSteps] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const queryClient = useQueryClient();

  useEffect(() => {
    if (isSuccess) {
      if (!task?.answer) {
        setIsLoading(true);
        // eslint-disable-next-line promise/catch-or-return
        service
          .askTask(taskId ?? "", (answer) => {
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
            if (answer.file_path) {
              setFilePath(answer.file_path);
            }
          })
          .finally(() => {
            setIsLoading(false);
            queryClient.invalidateQueries({
              queryKey: queryKeys.thread.all,
            });
          });
      }
    }
  }, [isSuccess, task, taskId, queryClient]);

  const answer = task?.answer ? task?.answer : answerStream;

  const showAnswer = answer || steps.length > 0;

  const showAgentThinking = isLoading && !showAnswer;
  const filePathB64 = filePath ? btoa(filePath) : undefined;

  return (
    <div className="flex flex-col h-full">
      <PageHeader className="border-b-1 border-border items-center">
        <div className="p-2 flex items-center justify-center flex-1 h-full">
          <div className="flex gap-1 items-center text-muted-foreground">
            <Bot className="w-4 h-4 min-w-4 min-h-4" />
            <p className="text-sm break-all">Builder</p>
          </div>
          <div className="px-4 h-full flex items-stretch">
            <Separator orientation="vertical" />
          </div>

          <p className="text-sm text-base-foreground">{task?.title}</p>
        </div>
      </PageHeader>

      <div className="flex flex-1 overflow-hidden">
        <div className="overflow-y-auto customScrollbar flex-1">
          <div className="flex-1 max-w-[742px] px-4 mx-auto pb-4">
            <div className="pt-8 pb-6 text-3xl font-semibold text-base-foreground">
              {task?.question}
            </div>

            {showAgentThinking && (
              <div className="flex gap-1">
                <img className="w-8 h-8" src="/oxy-loading-dark.gif" />
                <p className="text-muted-foreground">Agent is thinking...</p>
              </div>
            )}
            {showAnswer && (
              <div className="p-6 rounded-xl bg-base-card border border-base-border shadow-sm flex flex-col gap-2 ">
                <div className="flex gap-1 items-center h-12 justify-start">
                  <img
                    className="w-[24px] h-[24px]"
                    src="/logo.svg"
                    alt="Oxy"
                  />
                  <p className="text-xl text-card-foreground font-semibold">
                    Answer
                  </p>
                </div>
                <ThreadSteps steps={steps} isLoading={isLoading} />

                <AnswerContent content={answer || ""} />
              </div>
            )}
          </div>
        </div>
        <div className="border-l flex-1 h-full">
          <EditorTab pathb64={filePathB64} />
        </div>
      </div>
    </div>
  );
};

const TaskPage = () => {
  const { taskId } = useParams();
  return <Task key={taskId} />;
};

export default TaskPage;
