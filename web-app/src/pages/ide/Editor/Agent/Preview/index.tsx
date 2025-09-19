import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { cx } from "class-variance-authority";
import { ArrowUp, Loader2 } from "lucide-react";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import Messages from "./Messages";
import useAgent from "@/hooks/api/agents/useAgent";
import EmptyState from "@/components/ui/EmptyState";
import useAskAgent from "@/hooks/messaging/agent";
import useAgentThreadStore from "@/stores/useAgentThread";
import { useState } from "react";
import ArtifactPanelContainer from "./ArtifactPanelContainer";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const getAgentNameFromPath = (path: string) => {
  const parts = path.split("/");
  return parts[parts.length - 1].split(".")[0].replace(/_/g, " ");
};

const AgentPreview = ({ agentPathb64 }: { agentPathb64: string }) => {
  const path = atob(agentPathb64);
  const { project, branchName } = useCurrentProjectBranch();
  const threadId = `${project.id}_${branchName}_${agentPathb64}`;
  const { data: agent } = useAgent(agentPathb64);
  const [question, setQuestion] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();
  const { getAgentThread } = useAgentThreadStore();
  const { sendMessage } = useAskAgent();

  const [selectedArtifactIds, setSelectedArtifactIds] = useState<string[]>([]);
  const handleArtifactClick = (id: string) => setSelectedArtifactIds([id]);

  const agentThread = getAgentThread(threadId);
  const { messages, isLoading } = agentThread;

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!question.trim() || isLoading) return;
    sendMessage(question, threadId, { isPreview: true, agentPathb64 });
    setQuestion("");
  };

  const agentName = agent?.name ?? getAgentNameFromPath(path);

  return (
    <div className="flex flex-col h-full justify-between overflow-hidden">
      <div className="flex-1 flex flex-col overflow-auto customScrollbar scrollbar-gutter-auto">
        <div className="flex flex-col gap-4 p-4">
          {messages.length === 0 ? (
            <EmptyState
              className="h-full"
              title="No messages yet"
              description={`Ask the ${agentName} agent a question to get started`}
            />
          ) : (
            <Messages
              messages={messages}
              onArtifactClick={handleArtifactClick}
            />
          )}
        </div>
      </div>
      <div className="p-4">
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
            onChange={(e) => setQuestion(e.target.value)}
            value={question}
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

      <ArtifactPanelContainer
        messages={messages}
        selectedIds={selectedArtifactIds}
        onSelect={setSelectedArtifactIds}
      />
    </div>
  );
};

export default AgentPreview;
