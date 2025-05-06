import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import useThreadMutation from "@/hooks/api/useThreadMutation";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import { cx } from "class-variance-authority";
import { ArrowRight, Loader2, SquareChartGantt } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import AgentsDropdown, { Agent } from "./AgentsDropdown";
import useTaskMutation from "@/hooks/api/useTaskMutation";
import { Toggle } from "@/components/ui/shadcn/toggle";

const ChatPanel = ({
  agent,
  onChangeAgent,
}: {
  agent: Agent | null;
  onChangeAgent: (agent: Agent) => void;
}) => {
  const navigate = useNavigate();
  const { mutate: createThread, isPending } = useThreadMutation((data) => {
    navigate(`/threads/${data.id}`);
  });

  const { mutate: createTask, isPending: isCreatingTask } = useTaskMutation(
    (data) => {
      navigate(`/tasks/${data.id}`);
    },
  );

  const [message, setMessage] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();
  const [isBuildMode, setIsBuildMode] = useState(false);

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!message) return;
    if (isBuildMode) {
      createTask({
        title: message,
        question: message,
      });
    } else {
      createThread({
        title: message,
        agent: agent?.id ?? "",
        question: message,
      });
    }
  };

  return (
    <form
      ref={formRef}
      onSubmit={handleFormSubmit}
      className="w-full max-w-[672px] flex p-2 flex-col gap-1 shadow-sm rounded-md border-2 mx-auto bg-sidebar-background"
    >
      <Textarea
        disabled={isPending || isCreatingTask}
        name="question"
        autoFocus
        onKeyDown={onKeyDown}
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        className={cx(
          "border-none shadow-none",
          "hover:border-none focus-visible:border-none focus-visible:shadow-none",
          "focus-visible:ring-0 focus-visible:ring-offset-0",
          "outline-none resize-none",
        )}
        placeholder={`Ask anything`}
      />
      <div className="flex justify-between">
        <div className="flex items-center justify-center">
          <Toggle
            className="border"
            onPressedChange={(value) => setIsBuildMode(value)}
            aria-label="Toggle builder"
          >
            <SquareChartGantt />
            Builder
          </Toggle>
        </div>
        <div className="flex gap-2 items-center">
          <AgentsDropdown
            onSelect={onChangeAgent}
            disabled={isBuildMode}
            agent={agent}
          />
          <Button disabled={!message || isPending || !agent} type="submit">
            {isPending ? <Loader2 className="animate-spin" /> : <ArrowRight />}
          </Button>
        </div>
      </div>
    </form>
  );
};

export default ChatPanel;
