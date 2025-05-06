import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import useThreadMutation from "@/hooks/api/useThreadMutation";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import { cx } from "class-variance-authority";
import {
  ArrowRight,
  Hammer,
  Loader2,
  MessageCircleQuestion,
} from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import AgentsDropdown, { Agent } from "./AgentsDropdown";
import useTaskMutation from "@/hooks/api/useTaskMutation";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/shadcn/toggle-group";

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
  const [mode, setMode] = useState<string>("ask");

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!message) return;
    if (mode === "build") {
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
      className="w-full max-w-[672px] flex p-2 flex-col gap-1 shadow-sm rounded-md border-2 mx-auto bg-secondary"
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
          <ToggleGroup
            type="single"
            defaultValue="ask"
            className="gap-1 p-1 bg-sidebar-background text-accent-main-000 rounded-md"
            onValueChange={setMode}
          >
            <ToggleGroupItem
              value="ask"
              className="data-[state=on]:border hover:text-special hover:bg-button-hover data-[state=on]:bg-button-hover  data-[state=on]:text-special border-accent-main-000 rounded-md"
            >
              <MessageCircleQuestion />
              <span>Ask</span>
            </ToggleGroupItem>
            <ToggleGroupItem
              value="build"
              className="data-[state=on]:border hover:text-special hover:bg-button-hover data-[state=on]:bg-button-hover  data-[state=on]:text-special border-accent-main-000 rounded-md"
            >
              <Hammer />
              <span>Build</span>
            </ToggleGroupItem>
          </ToggleGroup>
        </div>
        <div className="flex gap-2 items-center">
          <AgentsDropdown
            onSelect={onChangeAgent}
            disabled={mode === "build"}
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
