import { ArrowUp, CircleX, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import useFileTree from "@/hooks/api/files/useFileTree";
import { cn } from "@/libs/shadcn/utils";
import { flattenFiles, getActiveMention, getCleanObjectName } from "@/libs/utils/mention";
import { getFileTypeIcon } from "@/pages/ide/Files/FilesSidebar/utils";
import type { FileTreeModel } from "@/types/file";
import { detectFileType } from "@/utils/fileTypes";
import { Button } from "../ui/shadcn/button";
import { Textarea } from "../ui/shadcn/textarea";

// --- Component ---

interface BuilderMessageInputProps {
  onSend: (resolvedText: string) => void;
  onStop: () => void;
  disabled: boolean;
  isLoading?: boolean;
  showWarning?: boolean;
  autoApprove?: boolean;
  onAutoApproveChange?: (value: boolean) => void;
  enableFileMentions?: boolean;
}

const BuilderMessageInput = ({
  onSend,
  onStop,
  disabled,
  isLoading = false,
  showWarning = false,
  autoApprove = false,
  onAutoApproveChange,
  enableFileMentions = true
}: BuilderMessageInputProps) => {
  const [message, setMessage] = useState("");
  const [cursorPos, setCursorPos] = useState(0);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [mentions, setMentions] = useState<Map<string, string>>(new Map());
  const textareaElRef = useRef<HTMLTextAreaElement | null>(null);

  const { data: fileTreeData } = useFileTree(enableFileMentions);

  const allFiles = useMemo(() => {
    if (!fileTreeData) return [];
    return flattenFiles(fileTreeData.primary);
  }, [fileTreeData]);

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

  const handleSend = () => {
    if (!message.trim() || disabled) return;
    onSend(resolveInput(message));
    setMessage("");
    setCursorPos(0);
    setMentions(new Map());
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
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
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
    <div className='flex w-full flex-1 flex-col gap-1.5'>
      {showWarning && (
        <div className='w-full rounded-lg border border-border bg-muted p-2'>
          <div className='flex items-center'>
            <svg
              aria-hidden='true'
              className='mr-2 h-5 w-5 text-destructive'
              fill='currentColor'
              viewBox='0 0 20 20'
            >
              <path
                fillRule='evenodd'
                d='M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z'
                clipRule='evenodd'
              />
            </svg>
            <span className='font-medium text-foreground text-sm'>
              You've asked a lot of questions. You may want to start a new thread for optimal
              performance.
            </span>
          </div>
        </div>
      )}
      <div className='relative'>
        {/* Mention autocomplete dropdown — rendered above the textarea */}
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
        <div className='overflow-hidden rounded-md border border-border bg-secondary transition-shadow focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50'>
          <Textarea
            ref={textareaRef}
            value={message}
            onChange={handleChange}
            onSelect={handleSelect}
            onClick={handleSelect}
            onKeyDown={handleKeyDown}
            placeholder='Ask a follow-up question... (@ to mention a file)'
            className='h-14 max-h-20 resize-none overflow-y-auto rounded-none border-0 bg-transparent shadow-none focus-visible:ring-0'
            disabled={disabled}
          />
          <div className='flex items-center justify-end gap-2 border-border border-t bg-secondary px-2 py-1.5'>
            {onAutoApproveChange && (
              <button
                type='button'
                onClick={() => onAutoApproveChange(!autoApprove)}
                className={cn(
                  "flex items-center gap-1 rounded px-1.5 py-0.5 text-xs transition-colors hover:bg-accent",
                  autoApprove ? "text-primary" : "text-muted-foreground"
                )}
              >
                <Zap className='h-3 w-3' />
                Auto-approve
              </button>
            )}
            {isLoading ? (
              <Button
                size='icon'
                className='size-7'
                onClick={onStop}
                data-testid='message-input-stop-button'
              >
                <CircleX className='size-4' />
              </Button>
            ) : (
              <Button
                size='icon'
                className='size-7'
                onClick={handleSend}
                disabled={!message.trim() || disabled}
                data-testid='message-input-send-button'
              >
                <ArrowUp className='size-4' />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default BuilderMessageInput;
