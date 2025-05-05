import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import useThreadMutation from "@/hooks/api/useThreadMutation";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import { cx } from "class-variance-authority";
import { ArrowRight, Loader2 } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import AgentsDropdown, { Agent } from "./AgentsDropdown";

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

  const [message, setMessage] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!message) return;
    createThread({
      title: message,
      agent: agent?.id ?? "",
      question: message,
    });
  };

  return (
    <form
      ref={formRef}
      onSubmit={handleFormSubmit}
      className="w-full max-w-[672px] flex p-2 flex-col gap-1 shadow-sm rounded-md border-2 mx-auto bg-sidebar-background"
    >
      <Textarea
        disabled={isPending}
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
        <AgentsDropdown onSelect={onChangeAgent} agent={agent} />
        <Button disabled={!message || isPending || !agent} type="submit">
          {isPending ? <Loader2 className="animate-spin" /> : <ArrowRight />}
        </Button>
      </div>
    </form>
  );
};

export default ChatPanel;
