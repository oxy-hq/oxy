import { ArrowUp, Hammer, MessageCircleQuestion, Play } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Select, SelectContent, SelectTrigger, SelectValue } from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Textarea } from "@/components/ui/shadcn/textarea";
import useThreadMutation from "@/hooks/api/threads/useThreadMutation";
import useBuilderAvailable from "@/hooks/api/useBuilderAvailable";
import useAskAgent from "@/hooks/messaging/agent";
import useAskTask from "@/hooks/messaging/task";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import useRunWorkflowThread from "@/hooks/workflow/useRunWorkflowThread";
import ROUTES from "@/libs/utils/routes";
import { getShortTitle } from "@/libs/utils/string";
import type { ThinkingMode } from "@/services/api/analytics";
import { useAskAgentic } from "@/stores/agentic";
import { setPendingThinkingMode } from "@/stores/analyticsThinkingMode";
import AgentsDropdown, { type Agent } from "./AgentsDropdown";
import SelectItemWithDetail from "./SelectItemWithDetail";
import WorkflowsDropdown, { type WorkflowOption } from "./WorkflowsDropdown";

const ChatPanel = () => {
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const { sendMessage } = useAskAgent();

  const { sendMessage: sendTaskMessage } = useAskTask();

  const { run: runWorkflow } = useRunWorkflowThread();

  const [agent, setAgent] = useState<Agent | null>(null);
  const [workflow, setWorkflow] = useState<WorkflowOption | null>(null);
  const { mutateAsync: sendAgenticMessage } = useAskAgentic();

  const { mutate: createThread, isPending } = useThreadMutation((data) => {
    switch (data.source_type) {
      case "agent":
        sendMessage(data.input, data.id);
        break;
      case "task":
        sendTaskMessage(data.input, data.id);
        break;
      case "agentic":
        sendAgenticMessage({
          prompt: data.input,
          threadId: data.id,
          agentRef: data.source
        });
        break;
      case "analytics":
        // Run creation is handled by AnalyticsThread's auto-start on first visit.
        // Do NOT create a run here — it races with auto-start and causes duplicates.
        setPendingThinkingMode(data.id, thinkingMode);
        break;
      case "workflow":
        runWorkflow(data.id);
        break;
    }
    const threadUrl = ROUTES.PROJECT(projectId).THREAD(data.id);
    navigate(threadUrl);
  });

  const {
    isAvailable: isBuilderAvailable,
    isLoading: isCheckingBuilder,
    isAgentic,
    builderPath
  } = useBuilderAvailable();

  const [message, setMessage] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();
  const [mode, setMode] = useState<string>("ask");
  const [thinkingMode, setThinkingMode] = useState<ThinkingMode>("auto");

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (isPending) return;
    const title = getShortTitle(message);

    switch (mode) {
      case "ask":
        if (!agent) return;
        createThread({
          title: title,
          source: agent.id,
          source_type: agent.isAnalytics ? "analytics" : agent.isAgentic ? "agentic" : "agent",
          input: message
        });
        break;
      case "build":
        if (isBuilderAvailable) {
          createThread({
            title: title,
            source: builderPath,
            source_type: isAgentic ? "agentic" : "task",
            input: message
          });
        }
        break;
      case "workflow":
        if (!workflow) return;
        createThread({
          title: title ? title : workflow.name,
          source: workflow.id,
          source_type: "workflow",
          input: message
        });
        break;
    }
  };

  const submitIcon = mode === "workflow" ? <Play /> : <ArrowUp />;
  const disabled = () => {
    if (isPending) return true;
    switch (mode) {
      case "ask":
        return !message || !agent;
      case "build":
        return !message || !isBuilderAvailable || isCheckingBuilder;
      case "workflow":
        return !workflow;
    }
  };

  const placeholder = (() => {
    switch (mode) {
      case "ask":
        return "Start your request, and let Oxygen handle everything.";
      case "build":
        return "Enter anything you want to build, and Oxygen will figure out the rest.";
      case "workflow":
        return "Enter a title for this procedure run.";
    }
  })();

  return (
    <form
      ref={formRef}
      onSubmit={handleFormSubmit}
      className='mx-auto flex w-full max-w-[672px] flex-col gap-1 rounded-md border bg-secondary p-3'
    >
      <Textarea
        disabled={isPending}
        name='question'
        autoFocus
        onKeyDown={onKeyDown}
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        className='max-h-[200px] resize-none border-none bg-transparent px-0 pt-[4px] pb-2 shadow-none outline-none hover:border-none focus-visible:border-none focus-visible:shadow-none focus-visible:ring-0 focus-visible:ring-offset-0'
        placeholder={placeholder}
      />

      <div className='flex justify-between'>
        <div className='flex items-center justify-center'>
          <Select value={mode} onValueChange={setMode}>
            <SelectTrigger size='sm' className='border-none bg-transparent'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItemWithDetail
                className='cursor-pointer'
                value='ask'
                detail={{
                  title: "Ask",
                  description:
                    "Interact in natural language to get instant insights. No SQL or technical knowledge required."
                }}
              >
                <MessageCircleQuestion className='size-4' />
                Ask
              </SelectItemWithDetail>
              <SelectItemWithDetail
                className='cursor-pointer'
                value='build'
                disabled={!isBuilderAvailable || isCheckingBuilder}
                detail={{
                  title: "Build",
                  description:
                    "Build data applications and dashboards by describing what you need in natural language."
                }}
              >
                <Hammer className='size-4' />
                Build
              </SelectItemWithDetail>
              <SelectItemWithDetail
                className='cursor-pointer'
                value='workflow'
                detail={{
                  title: "Procedure",
                  description:
                    "Automate multi-step workflows with intelligent agents that execute complex processes autonomously."
                }}
              >
                <Play className='size-4' />
                Procedure
              </SelectItemWithDetail>
            </SelectContent>
          </Select>
        </div>
        <div className='flex items-center gap-2'>
          {mode === "ask" && (
            <AgentsDropdown
              onSelect={(a) => {
                if (!a.isAnalytics) setThinkingMode("auto");
                setAgent(a);
              }}
              agentSelected={agent}
              thinkingMode={thinkingMode}
              onThinkingModeChange={setThinkingMode}
              disabled={isPending}
            />
          )}
          {mode === "workflow" && <WorkflowsDropdown onSelect={setWorkflow} workflow={workflow} />}
          <Button
            size='sm'
            disabled={disabled()}
            type='submit'
            data-testid='chat-panel-submit-button'
          >
            {isPending ? <Spinner /> : submitIcon}
          </Button>
        </div>
      </div>
    </form>
  );
};

export default ChatPanel;
