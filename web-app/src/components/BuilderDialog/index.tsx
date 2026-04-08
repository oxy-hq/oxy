import { Hammer, Loader2, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import useFileTree from "@/hooks/api/files/useFileTree";
import useThread from "@/hooks/api/threads/useThread";
import useThreadMutation from "@/hooks/api/threads/useThreadMutation";
import useBuilderAvailable from "@/hooks/api/useBuilderAvailable";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import { cn } from "@/libs/shadcn/utils";
import { flattenFiles, getActiveMention, getCleanObjectName } from "@/libs/utils/mention";
import ROUTES from "@/libs/utils/routes";
import { getShortTitle } from "@/libs/utils/string";
import { getFileTypeIcon } from "@/pages/ide/Files/FilesSidebar/utils";
import { AnalyticsService } from "@/services/api";
import { useAskAgentic } from "@/stores/agentic";
import useBuilderDialog from "@/stores/useBuilderDialog";
import type { FileTreeModel } from "@/types/file";
import { decodeFilePath, detectFileType } from "@/utils/fileTypes";
import { Dialog, DialogContent } from "../ui/shadcn/dialog";
import { Textarea } from "../ui/shadcn/textarea";

// --- Helpers ---

function extractFileContext(pathname: string) {
  const match = pathname.match(/(?:\/ide\/files|\/apps|\/workflows)\/([^/]+)/);
  if (!match) return null;
  const pathb64 = match[1];
  const filePath = decodeFilePath(pathb64);
  if (!filePath) return null;
  const fileName = filePath.split("/").pop() || filePath;
  const fileType = detectFileType(filePath);
  const displayName = getCleanObjectName(fileName);
  const Icon = getFileTypeIcon(fileType, fileName);
  return { pathb64, filePath, fileName, displayName, fileType, Icon };
}

// --- Component ---

export function BuilderDialog() {
  const { isOpen, setIsOpen } = useBuilderDialog();
  const navigate = useNavigate();
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [autoApprove, setAutoApprove] = useState(
    () => localStorage.getItem("builder_auto_approve") === "true"
  );
  const [message, setMessage] = useState("");
  const [cursorPos, setCursorPos] = useState(0);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [mentions, setMentions] = useState<Map<string, string>>(new Map()); // displayName → filePath
  const { formRef, onKeyDown: enterSubmitKeyDown } = useEnterSubmit();
  const { mutateAsync: sendAgenticMessage } = useAskAgentic();
  const textareaElRef = useRef<HTMLTextAreaElement | null>(null);
  const textareaRafRef = useRef<number | null>(null);
  const insertMentionRafRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (textareaRafRef.current !== null) cancelAnimationFrame(textareaRafRef.current);
      if (insertMentionRafRef.current !== null) cancelAnimationFrame(insertMentionRafRef.current);
    };
  }, []);

  const {
    isAvailable,
    isLoading: isCheckingBuilder,
    isAgentic,
    isBuiltin,
    builderModel,
    builderPath
  } = useBuilderAvailable();

  const { data: fileTreeData } = useFileTree(isOpen);

  const allFiles = useMemo(() => {
    if (!fileTreeData) return [];
    return flattenFiles(fileTreeData.primary);
  }, [fileTreeData]);

  const fileContext = useMemo(() => extractFileContext(location.pathname), [location.pathname]);
  const fileDisplayName = fileContext?.displayName;

  // Extract thread ID if we're on a thread page
  const threadId = useMemo(() => {
    const match = location.pathname.match(/\/threads\/([^/]+)/);
    return match ? match[1] : null;
  }, [location.pathname]);

  const { data: threadData } = useThread(threadId ?? "", isOpen && !!threadId && !fileContext);

  // Mention autocomplete state
  const activeMention = getActiveMention(message, cursorPos);
  const mentionResults = useMemo(() => {
    if (!activeMention) return [];
    const q = activeMention.query.toLowerCase();
    return allFiles
      .filter((f) => {
        const name = f.name.toLowerCase();
        const path = f.path.toLowerCase();
        return name.includes(q) || path.includes(q);
      })
      .slice(0, 8);
  }, [activeMention, allFiles]);

  const showMentionPopup = activeMention !== null && mentionResults.length > 0;

  // Reset selected index when results change
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset on result count change only
  useEffect(() => {
    setSelectedIndex(0);
  }, [mentionResults.length]);

  const textareaRef = useCallback(
    (node: HTMLTextAreaElement | null) => {
      if (textareaRafRef.current !== null) {
        cancelAnimationFrame(textareaRafRef.current);
        textareaRafRef.current = null;
      }
      textareaElRef.current = node;
      if (node && isOpen && message) {
        textareaRafRef.current = requestAnimationFrame(() => {
          textareaRafRef.current = null;
          const len = node.value.length;
          node.setSelectionRange(len, len);
        });
      }
    },
    [isOpen, message]
  );

  // Pre-fill @mention when dialog opens with file context or thread agent
  useEffect(() => {
    if (isOpen && fileDisplayName && fileContext) {
      setMessage(`@${fileDisplayName} `);
      setMentions(new Map([[fileDisplayName, fileContext.filePath]]));
    } else if (isOpen && threadData?.source && threadData.source !== "__builder__") {
      const agentFile = allFiles.find((f) => f.path === threadData.source);
      const displayName = agentFile
        ? getCleanObjectName(agentFile.name)
        : getCleanObjectName(threadData.source.split("/").pop() ?? threadData.source);
      const filePath = agentFile?.path ?? threadData.source;
      setMessage(`@${displayName} `);
      setMentions(new Map([[displayName, filePath]]));
    }
    if (!isOpen) {
      setMessage("");
      setCursorPos(0);
      setMentions(new Map());
    }
  }, [isOpen, fileDisplayName, fileContext, threadData, allFiles]);

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
    // Restore focus and cursor
    if (insertMentionRafRef.current !== null) cancelAnimationFrame(insertMentionRafRef.current);
    insertMentionRafRef.current = requestAnimationFrame(() => {
      insertMentionRafRef.current = null;
      const el = textareaElRef.current;
      if (el) {
        el.focus();
        el.setSelectionRange(newCursorPos, newCursorPos);
      }
    });
  };

  const { mutate: createThread, isPending } = useThreadMutation((data) => {
    switch (data.source_type) {
      case "agentic":
        sendAgenticMessage({
          prompt: data.input,
          threadId: data.id,
          agentRef: data.source
        });
        break;
      case "analytics":
        AnalyticsService.createRun(projectId, {
          agent_id: data.source,
          question: data.input,
          thread_id: data.id,
          ...(data.source === "__builder__" && {
            domain: "builder",
            model: builderModel
          })
        });
        break;
    }
    setIsOpen(false);
    setMessage("");
    const threadUri = ROUTES.WORKSPACE(projectId).THREAD(data.id);
    navigate(threadUri);
  });

  const resolveInput = (text: string) => {
    let resolved = text;
    for (const [displayName, filePath] of mentions) {
      resolved = resolved.replaceAll(`@${displayName}`, `<${filePath}>`);
    }
    return resolved;
  };

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!message.trim() || !isAvailable || isCheckingBuilder) return;

    const input = resolveInput(message);
    const title = getShortTitle(message);

    if (isBuiltin) {
      createThread({
        title,
        source: "__builder__",
        source_type: "analytics",
        input
      });
    } else {
      createThread({
        title,
        source: builderPath,
        source_type: isAgentic ? "agentic" : "task",
        input
      });
    }
  };

  const handleOpenChange = (open: boolean) => {
    setIsOpen(open);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
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
        // Close mention popup by moving cursor, don't close dialog
        return;
      }
    }
    if (e.key === "Backspace") {
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

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setMessage(e.target.value);
    setCursorPos(e.target.selectionStart ?? e.target.value.length);
  };

  const handleSelect = (e: React.SyntheticEvent<HTMLTextAreaElement>) => {
    const target = e.target as HTMLTextAreaElement;
    setCursorPos(target.selectionStart ?? 0);
  };

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent
        showCloseButton={false}
        className='top-[30%] max-w-150 translate-y-0 gap-0 overflow-hidden p-0'
      >
        {!isAvailable && !isCheckingBuilder ? (
          <div className='p-6 text-center text-muted-foreground text-sm'>
            Builder is not available for this project.
          </div>
        ) : (
          <form ref={formRef} onSubmit={handleSubmit} className='flex flex-col'>
            {/* Top bar */}
            <div className='flex items-center gap-2 border-b px-3 py-2'>
              <span className='inline-flex items-center gap-1 rounded-md bg-orange-500/15 px-2 py-0.5 font-medium text-orange-500 text-xs'>
                <Hammer className='size-3' />
                Build
              </span>
              <span className='text-muted-foreground text-xs'>
                <kbd className='rounded border bg-muted px-1 py-0.5 font-mono text-[10px]'>@</kbd>{" "}
                to mention
              </span>
              <span className='ml-auto text-muted-foreground text-xs'>
                <kbd className='rounded border bg-muted px-1 py-0.5 font-mono text-[10px]'>
                  Enter
                </kbd>{" "}
                to build
              </span>
            </div>

            {/* Textarea */}
            <Textarea
              ref={textareaRef}
              autoFocus
              name='builder-input'
              disabled={isPending}
              onKeyDown={handleKeyDown}
              value={message}
              onChange={handleChange}
              onSelect={handleSelect}
              onClick={handleSelect}
              className='customScrollbar max-h-50 min-h-20 resize-none border-none bg-transparent px-3 py-3 text-sm shadow-none outline-none focus-visible:ring-0 focus-visible:ring-offset-0'
              placeholder='Describe what you want to build...'
            />

            {/* Mention autocomplete dropdown */}
            {showMentionPopup && (
              <div className='max-h-52 overflow-y-auto border-t bg-popover p-1'>
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

            {/* Footer */}
            <div className='flex items-center justify-between border-t px-3 py-2'>
              <div className='flex items-center gap-2'>
                {isPending && <Loader2 className='size-3.5 animate-spin text-muted-foreground' />}
                <span className='text-muted-foreground text-xs'>
                  Press{" "}
                  <kbd className='rounded border bg-muted px-1 py-0.5 font-mono text-[10px]'>
                    Esc
                  </kbd>{" "}
                  to cancel.
                </span>
              </div>
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
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
