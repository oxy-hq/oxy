import { cx } from "class-variance-authority";
import { ArrowRight, Hammer, Loader2, MessageCircleQuestion, Play, Workflow } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";
import useThreadMutation from "@/hooks/api/threads/useThreadMutation";
import useBuilderAvailable from "@/hooks/api/useBuilderAvailable";
import useAskAgent from "@/hooks/messaging/agent";
import useAskTask from "@/hooks/messaging/task";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import useRunWorkflowThread from "@/hooks/workflow/useRunWorkflowThread";
import ROUTES from "@/libs/utils/routes";
import { getShortTitle } from "@/libs/utils/string";
import { useAskAgentic } from "@/stores/agentic";
import AgentsDropdown, { type Agent } from "./AgentsDropdown";
import WorkflowsDropdown, { type WorkflowOption } from "./WorkflowsDropdown";

const ToggleGroupItemClasses =
  "data-[state=on]:border data-[state=on]:border-blue-500 data-[state=on]:bg-blue-500 data-[state=on]:text-white hover:bg-blue-500/20 hover:text-blue-300 hover:border-blue-400/50 transition-colors border-gray-600 rounded-md text-gray-400";

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
      case "workflow":
        runWorkflow(data.id);
        break;
    }
    const threadUri = ROUTES.PROJECT(projectId).THREAD(data.id);
    navigate(threadUri);
  });

  const {
    isAvailable: isBuilderAvailable,
    isLoading: isCheckingBuilder,
    isAgentic
  } = useBuilderAvailable();

  const [message, setMessage] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();
  const [mode, setMode] = useState<string>("ask");

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const title = getShortTitle(message);

    switch (mode) {
      case "ask":
        if (!agent) return;
        createThread({
          title: title,
          source: agent.id,
          source_type: agent.isAgentic ? "agentic" : "agent",
          input: message
        });
        break;
      case "build":
        if (isBuilderAvailable) {
          createThread({
            title: title,
            source: "",
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

  const submitIcon = mode === "workflow" ? <Play /> : <ArrowRight />;
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
        return "Ask anything";
      case "build":
        return "Enter anything you want to build";
      case "workflow":
        return "Enter a title for this workflow run";
    }
  })();

  return (
    <form
      ref={formRef}
      onSubmit={handleFormSubmit}
      className='mx-auto flex w-full max-w-[672px] flex-col gap-1 rounded-md border-2 bg-secondary p-2 shadow-sm'
    >
      <Textarea
        disabled={isPending}
        name='question'
        autoFocus
        onKeyDown={onKeyDown}
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        className={cx(
          "border-none shadow-none",
          "hover:border-none focus-visible:border-none focus-visible:shadow-none",
          "focus-visible:ring-0 focus-visible:ring-offset-0",
          "customScrollbar max-h-[200px] resize-none px-0 outline-none"
        )}
        placeholder={placeholder}
      />

      <div className='flex justify-between'>
        <div className='flex items-center justify-center'>
          <ToggleGroup
            size='sm'
            type='single'
            value={mode}
            className='gap-1 rounded-md bg-sidebar-background p-1 text-accent-main-000'
            onValueChange={(value) => {
              if (value) {
                setMode(value);
              }
            }}
          >
            <ToggleGroupItem size='sm' value='ask' className={ToggleGroupItemClasses}>
              <MessageCircleQuestion />
              <span>Ask</span>
            </ToggleGroupItem>
            <ToggleGroupItem
              size='sm'
              value='build'
              className={ToggleGroupItemClasses}
              disabled={!isBuilderAvailable || isCheckingBuilder}
              title={!isBuilderAvailable ? "Builder agent not available" : ""}
            >
              <Hammer />
              <span>Build</span>
            </ToggleGroupItem>
            <ToggleGroupItem size='sm' value='workflow' className={ToggleGroupItemClasses}>
              <Workflow />
              <span>Workflow</span>
            </ToggleGroupItem>
          </ToggleGroup>
        </div>
        <div className='flex items-center gap-2'>
          {mode === "ask" && <AgentsDropdown onSelect={setAgent} agentSelected={agent} />}
          {mode === "workflow" && <WorkflowsDropdown onSelect={setWorkflow} workflow={workflow} />}
          <Button disabled={disabled()} type='submit' data-testid='chat-panel-submit-button'>
            {isPending ? <Loader2 className='animate-spin' /> : submitIcon}
          </Button>
        </div>
      </div>
    </form>
  );
};

export default ChatPanel;
