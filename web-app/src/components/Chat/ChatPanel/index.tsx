import { ArrowUp, Hammer, MessageCircleQuestion, Play, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Select, SelectContent, SelectTrigger, SelectValue } from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Textarea } from "@/components/ui/shadcn/textarea";
import useFileTree from "@/hooks/api/files/useFileTree";
import useThreadMutation from "@/hooks/api/threads/useThreadMutation";
import useBuilderAvailable from "@/hooks/api/useBuilderAvailable";
import useAskAgent from "@/hooks/messaging/agent";
import useAskTask from "@/hooks/messaging/task";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import useRunWorkflowThread from "@/hooks/workflow/useRunWorkflowThread";
import { cn } from "@/libs/shadcn/utils";
import { flattenFiles, getActiveMention, getCleanObjectName } from "@/libs/utils/mention";
import ROUTES from "@/libs/utils/routes";
import { getShortTitle } from "@/libs/utils/string";
import { getFileTypeIcon } from "@/pages/ide/Files/FilesSidebar/utils";
import type { ThinkingMode } from "@/services/api/analytics";
import { useAskAgentic } from "@/stores/agentic";
import { setPendingThinkingMode } from "@/stores/analyticsThinkingMode";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { FileTreeModel } from "@/types/file";
import { detectFileType } from "@/utils/fileTypes";
import AgentsDropdown, { type Agent } from "./AgentsDropdown";
import SelectItemWithDetail from "./SelectItemWithDetail";
import WorkflowsDropdown, { type WorkflowOption } from "./WorkflowsDropdown";

const ChatPanel = ({
  initialMessage,
  initialAgentPath,
  autoSubmit
}: {
  initialMessage?: string;
  initialAgentPath?: string;
  autoSubmit?: boolean;
}) => {
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const { sendMessage } = useAskAgent();

  const { sendMessage: sendTaskMessage } = useAskTask();

  const { run: runWorkflow } = useRunWorkflowThread();

  const [agent, setAgent] = useState<Agent | null>(null);
  const [workflow, setWorkflow] = useState<WorkflowOption | null>(null);
  const { mutateAsync: sendAgenticMessage } = useAskAgentic();

  const {
    isAvailable: isBuilderAvailable,
    isLoading: isCheckingBuilder,
    isAgentic,
    isBuiltin,
    builderPath
  } = useBuilderAvailable();

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
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).THREAD(data.id));
  });

  const autoSubmitDone = useRef(false);

  const [autoApprove, setAutoApprove] = useState(
    () => localStorage.getItem("builder_auto_approve") === "true"
  );
  const [message, setMessage] = useState(initialMessage ?? "");

  // Auto-submit when navigated with prefilled question + agent (e.g. from onboarding)
  // biome-ignore lint/correctness/useExhaustiveDependencies: formRef is a stable ref
  useEffect(() => {
    if (autoSubmit && !autoSubmitDone.current && message && agent && !isPending) {
      autoSubmitDone.current = true;
      formRef.current?.requestSubmit();
    }
  }, [autoSubmit, message, agent, isPending]);

  const [cursorPos, setCursorPos] = useState(0);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [mentions, setMentions] = useState<Map<string, string>>(new Map());
  const textareaElRef = useRef<HTMLTextAreaElement | null>(null);
  const { formRef, onKeyDown: enterSubmitKeyDown } = useEnterSubmit();
  const [mode, setMode] = useState<string>("ask");
  const [thinkingMode, setThinkingMode] = useState<ThinkingMode>("auto");

  const isBuildMode = mode === "build" && isBuiltin;

  const { data: fileTreeData } = useFileTree(isBuildMode);
  const allFiles = useMemo(() => {
    if (!fileTreeData) return [];
    return flattenFiles(fileTreeData.primary);
  }, [fileTreeData]);

  const activeMention = isBuildMode ? getActiveMention(message, cursorPos) : null;
  const mentionResults = useMemo(() => {
    if (!activeMention) return [];
    const q = activeMention.query.toLowerCase();
    return allFiles
      .filter((f) => f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q))
      .slice(0, 8);
  }, [activeMention, allFiles]);
  const showMentionPopup = activeMention !== null && mentionResults.length > 0;

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset on result count change only
  useEffect(() => {
    setSelectedIndex(0);
  }, [mentionResults.length]);

  const textareaRef = useCallback((node: HTMLTextAreaElement | null) => {
    textareaElRef.current = node;
  }, []);

  const insertMention = (file: FileTreeModel) => {
    if (!activeMention) return;
    const before = message.slice(0, activeMention.startIndex);
    const after = message.slice(cursorPos);
    const displayName = getCleanObjectName(file.name);
    const mention = `@${displayName}`;
    const newMessage = `${before}${mention} ${after}`;
    setMessage(newMessage);
    setMentions((prev) => new Map(prev).set(displayName, file.path));
    const newCursorPos = before.length + mention.length + 1;
    setCursorPos(newCursorPos);
    requestAnimationFrame(() => {
      const el = textareaElRef.current;
      if (el) {
        el.focus();
        el.setSelectionRange(newCursorPos, newCursorPos);
      }
    });
  };

  const resolveInput = (text: string) => {
    let resolved = text;
    for (const [displayName, filePath] of mentions) {
      resolved = resolved.replaceAll(`@${displayName}`, `<${filePath}>`);
    }
    return resolved;
  };

  const handleTextareaKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (showMentionPopup) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % mentionResults.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + mentionResults.length) % mentionResults.length);
        return;
      }
      if (e.key === "Tab" || e.key === "Enter") {
        e.preventDefault();
        insertMention(mentionResults[selectedIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        return;
      }
    }
    if (e.key === "Backspace" && isBuildMode) {
      const before = message.slice(0, cursorPos);
      for (const [displayName] of mentions) {
        const withSpace = `@${displayName} `;
        const withoutSpace = `@${displayName}`;
        const removeLen = before.endsWith(withSpace)
          ? withSpace.length
          : before.endsWith(withoutSpace)
            ? withoutSpace.length
            : 0;
        if (removeLen > 0) {
          e.preventDefault();
          const newCursorPos = cursorPos - removeLen;
          setMessage(message.slice(0, newCursorPos) + message.slice(cursorPos));
          setCursorPos(newCursorPos);
          setMentions((prev) => {
            const next = new Map(prev);
            next.delete(displayName);
            return next;
          });
          requestAnimationFrame(() => {
            const el = textareaElRef.current;
            if (el) el.setSelectionRange(newCursorPos, newCursorPos);
          });
          return;
        }
      }
    }
    enterSubmitKeyDown(e);
  };

  const handleTextareaChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setMessage(e.target.value);
    if (isBuildMode) setCursorPos(e.target.selectionStart ?? e.target.value.length);
  };

  const handleTextareaSelect = (e: React.SyntheticEvent<HTMLTextAreaElement>) => {
    if (isBuildMode) setCursorPos((e.target as HTMLTextAreaElement).selectionStart ?? 0);
  };

  const handleFormSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (isPending) return;
    const input = isBuildMode ? resolveInput(message) : message;
    const title = getShortTitle(message);

    switch (mode) {
      case "ask":
        if (!agent) return;
        createThread({
          title: title,
          source: agent.id,
          source_type: agent.isAnalytics ? "analytics" : agent.isAgentic ? "agentic" : "agent",
          input
        });
        break;
      case "build":
        if (isBuilderAvailable) {
          if (isBuiltin) {
            createThread({
              title: title,
              source: "__builder__",
              source_type: "analytics",
              input
            });
          } else {
            createThread({
              title: title,
              source: builderPath,
              source_type: isAgentic ? "agentic" : "task",
              input
            });
          }
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
      className='relative mx-auto flex w-full max-w-[672px] flex-col gap-1 rounded-md border bg-secondary p-2'
    >
      {showMentionPopup && (
        <div className='absolute right-0 bottom-full left-0 z-10 mb-1 max-h-52 overflow-y-auto rounded-md border bg-popover p-1 shadow-md'>
          {mentionResults.map((file, index) => {
            const fileType = detectFileType(file.path);
            const FileIcon = getFileTypeIcon(fileType, file.name);
            return (
              <button
                key={file.path}
                type='button'
                className={cn(
                  "flex w-full cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden",
                  index === selectedIndex
                    ? "bg-accent text-accent-foreground"
                    : "text-popover-foreground"
                )}
                onMouseDown={(e) => {
                  e.preventDefault();
                  insertMention(file);
                }}
                onMouseEnter={() => setSelectedIndex(index)}
              >
                {FileIcon && <FileIcon className='size-4 text-muted-foreground' />}
                <span className='flex-1 truncate text-left'>{file.path}</span>
              </button>
            );
          })}
        </div>
      )}
      <Textarea
        ref={textareaRef}
        disabled={isPending}
        name='question'
        autoFocus
        onKeyDown={handleTextareaKeyDown}
        value={message}
        onChange={handleTextareaChange}
        onSelect={handleTextareaSelect}
        onClick={handleTextareaSelect}
        className='customScrollbar max-h-[200px] resize-none border-none bg-transparent px-0 shadow-none outline-none hover:border-none focus-visible:border-none focus-visible:shadow-none focus-visible:ring-0 focus-visible:ring-offset-0'
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
              preferAgentPath={initialAgentPath}
              thinkingMode={thinkingMode}
              onThinkingModeChange={setThinkingMode}
              disabled={isPending}
            />
          )}
          {mode === "workflow" && <WorkflowsDropdown onSelect={setWorkflow} workflow={workflow} />}
          {isBuildMode && (
            <button
              type='button'
              onClick={() => {
                const next = !autoApprove;
                setAutoApprove(next);
                localStorage.setItem("builder_auto_approve", String(next));
              }}
              className={cn(
                "flex items-center gap-1 rounded px-1.5 py-0.5 text-xs transition-colors hover:bg-accent",
                autoApprove ? "text-primary" : "text-muted-foreground"
              )}
            >
              <Zap className='h-3 w-3' />
              Auto-approve
            </button>
          )}
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
