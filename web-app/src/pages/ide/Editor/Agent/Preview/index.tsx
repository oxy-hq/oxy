import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { cx } from "class-variance-authority";
import { ArrowUp, Loader2 } from "lucide-react";
import { useState } from "react";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import { service } from "@/services/service";
import Messages from "./Messages";
import useAgent from "@/hooks/api/useAgent";
import { Message } from "@/types/chat";

const getAgentNameFromPath = (path: string) => {
  const parts = path.split("/");
  return parts[parts.length - 1].split(".")[0].replace(/_/g, " ");
};

const STEP_MAP = {
  execute_sql: "Execute SQL",
  visualize: "Generate visualization",
  retrieve: "Retrieve data",
};

const AgentPreview = ({ agentPathb64 }: { agentPathb64: string }) => {
  const path = atob(agentPathb64);
  const { data: agent } = useAgent(agentPathb64);
  const [question, setQuestion] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();
  const [messages, setMessages] = useState<Message[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!question) return;
    setIsLoading(true);
    setMessages((prev) => [
      ...prev,
      {
        content: question,
        references: [],
        steps: [],
        isUser: true,
        isStreaming: false,
      },
      {
        content: "",
        references: [],
        steps: [],
        isUser: false,
        isStreaming: true,
      },
    ]);
    return service
      .askAgent(agentPathb64, question, (answer) => {
        setMessages((prev) => {
          const currentStreamingMessage = prev.at(-1);
          const { steps, references, content } = currentStreamingMessage ?? {
            content: "",
            references: [],
            steps: [],
            isUser: false,
            isStreaming: true,
          };

          const shouldAddStep =
            answer.step &&
            Object.keys(STEP_MAP).includes(answer.step) &&
            steps.at(-1) !== answer.step;

          const updatedMessages = [...prev];

          updatedMessages[prev.length - 1] = {
            content: content + answer.content,
            references: answer.references
              ? [...references, ...answer.references]
              : references,
            steps: shouldAddStep ? [...steps, answer.step] : steps,
            isUser: false,
            isStreaming: true,
          };
          return updatedMessages;
        });
      })
      .catch((error) => {
        console.error("Error asking agent:", error);
      })
      .finally(() => {
        setMessages((prev) => {
          const lastMessage = prev.at(-1);
          if (lastMessage?.isStreaming) {
            lastMessage.isStreaming = false;
          }
          return prev;
        });
        setIsLoading(false);
        setQuestion("");
      });
  };

  const agentName = agent?.name ?? getAgentNameFromPath(path);

  return (
    <div className="flex flex-col h-full justify-between overflow-hidden px-4 pb-4">
      <div className="flex flex-col gap-4 flex-1 overflow-auto customScrollbar py-6">
        <Messages messages={messages} />
      </div>
      <form
        ref={formRef}
        onSubmit={handleFormSubmit}
        className="w-full max-w-[672px] flex p-2 flex gap-1 shadow-sm rounded-md border-2 mx-auto"
      >
        <Textarea
          disabled={isLoading}
          name="question"
          autoFocus
          onKeyDown={onKeyDown}
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          className={cx(
            "border-none shadow-none",
            "hover:border-none focus-visible:border-none focus-visible:shadow-none",
            "focus-visible:ring-0 focus-visible:ring-offset-0",
            "outline-none resize-none",
            "min-h-[32px] box-border",
          )}
          placeholder={`Ask the ${agentName} agent a question`}
        />
        <Button className="w-8 h-8" disabled={!question} type="submit">
          {isLoading ? <Loader2 className="animate-spin" /> : <ArrowUp />}
        </Button>
      </form>
    </div>
  );
};

export default AgentPreview;
