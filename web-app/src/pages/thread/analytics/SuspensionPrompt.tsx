import { ChevronLeft, ChevronRight, MessageSquare } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { cn } from "@/libs/shadcn/utils";
import type { HumanInputQuestion } from "@/services/api/analytics";
import ProposeChangeDiff, { parseProposeChange } from "./ProposeChangeDiff";

interface SuspensionPromptProps {
  questions: HumanInputQuestion[];
  onAnswer: (text: string) => void;
  isAnswering: boolean;
}

const SuspensionPrompt = ({ questions, onAnswer, isAnswering }: SuspensionPromptProps) => {
  const [answers, setAnswers] = useState<string[]>(() => questions.map(() => ""));
  const [activeIndex, setActiveIndex] = useState(0);
  const safeIndex = Math.min(activeIndex, Math.max(0, questions.length - 1));
  const activeQuestion = questions[safeIndex];

  const setAnswer = (index: number, value: string) => {
    setAnswers((prev) => {
      const next = [...prev];
      next[index] = value;
      return next;
    });
  };

  const submitAll = () => {
    const combined =
      questions.length === 1
        ? answers[0].trim()
        : questions.map((q, i) => `Q: ${q.prompt}\nA: ${answers[i].trim()}`).join("\n\n");
    if (!combined) return;
    onAnswer(combined);
    setAnswers(questions.map(() => ""));
  };

  const allFilled = answers.every((a) => a.trim().length > 0);

  const nextUnansweredIndex = (() => {
    // Search after current, then wrap around from beginning
    for (let i = safeIndex + 1; i < questions.length; i++) {
      if (!answers[i].trim()) return i;
    }
    for (let i = 0; i < safeIndex; i++) {
      if (!answers[i].trim()) return i;
    }
    return null;
  })();

  const handleNextOrSubmit = () => {
    if (allFilled) {
      submitAll();
    } else if (nextUnansweredIndex !== null) {
      setActiveIndex(nextUnansweredIndex);
    }
  };

  const isNextOrSubmitDisabled = isAnswering || (!answers[safeIndex].trim() && !allFilled);

  const nextOrSubmitLabel = allFilled ? "Send" : "Next";

  // Single propose_change question → render the diff UI instead of the card.
  if (questions.length === 1) {
    const payload = parseProposeChange(questions[0].prompt);
    if (payload) {
      return (
        <div className='rounded-lg border border-border bg-muted/30 p-4'>
          <ProposeChangeDiff
            payload={payload}
            suggestions={questions[0].suggestions}
            onAnswer={onAnswer}
            isAnswering={isAnswering}
          />
        </div>
      );
    }
  }

  return (
    <div className='rounded-lg border border-border bg-muted/30'>
      {/* Card navigation header */}
      <div className='flex items-center gap-2 border-border border-b px-3 py-1.5'>
        <MessageSquare className='h-3 w-3 shrink-0 text-muted-foreground' />
        <span className='flex-1 text-muted-foreground text-sm'>
          {questions.length === 1 ? "Question" : `${questions.length} questions`}
        </span>

        {questions.length > 1 && (
          <div className='flex items-center gap-1'>
            <button
              type='button'
              onClick={() => setActiveIndex((i) => Math.max(0, i - 1))}
              disabled={safeIndex === 0}
              className='rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30'
              aria-label='Previous question'
            >
              <ChevronLeft className='h-3.5 w-3.5' />
            </button>
            <span className='min-w-[3rem] text-center font-mono text-muted-foreground text-xs'>
              {safeIndex + 1} / {questions.length}
            </span>
            <button
              type='button'
              onClick={() => setActiveIndex((i) => Math.min(questions.length - 1, i + 1))}
              disabled={safeIndex >= questions.length - 1}
              className='rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30'
              aria-label='Next question'
            >
              <ChevronRight className='h-3.5 w-3.5' />
            </button>
          </div>
        )}
      </div>

      {/* Active question content */}
      <div className='space-y-3 p-3'>
        <p className='font-medium text-sm'>{activeQuestion.prompt}</p>

        {activeQuestion.suggestions.length > 0 && (
          <div className='flex flex-wrap gap-2'>
            {activeQuestion.suggestions.map((s) => (
              <button
                key={s}
                type='button'
                onClick={() => {
                  if (questions.length === 1) {
                    onAnswer(s);
                  } else {
                    setAnswer(safeIndex, s);
                  }
                }}
                disabled={isAnswering}
                className='rounded-full border border-border px-3 py-1 text-xs transition-colors hover:bg-accent disabled:opacity-50'
              >
                {s}
              </button>
            ))}
          </div>
        )}

        <div className='flex gap-2'>
          <Textarea
            value={answers[safeIndex]}
            onChange={(e) => setAnswer(safeIndex, e.target.value)}
            placeholder='Type your answer…'
            className='min-h-[60px] flex-1 resize-none text-sm'
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                if (questions.length === 1) {
                  submitAll();
                } else {
                  handleNextOrSubmit();
                }
              }
            }}
            disabled={isAnswering}
          />
          {questions.length === 1 ? (
            <Button onClick={submitAll} disabled={isAnswering || !allFilled} className='self-end'>
              Send
            </Button>
          ) : (
            <Button
              onClick={handleNextOrSubmit}
              disabled={isNextOrSubmitDisabled}
              className='self-end'
            >
              {nextOrSubmitLabel}
            </Button>
          )}
        </div>
      </div>

      {/* Dot navigation */}
      {questions.length > 1 && (
        <div className='flex justify-center gap-1.5 px-3 pb-3'>
          {questions.map((_, i) => (
            <button
              // biome-ignore lint/suspicious/noArrayIndexKey: index is stable for fixed-count dots
              key={i}
              type='button'
              onClick={() => setActiveIndex(i)}
              aria-label={`Question ${i + 1}`}
              className={cn(
                "h-1.5 rounded-full transition-all",
                i === safeIndex
                  ? "w-4 bg-primary"
                  : answers[i].trim()
                    ? "w-1.5 bg-primary/40 hover:bg-primary/60"
                    : "w-1.5 bg-muted-foreground/30 hover:bg-muted-foreground/60"
              )}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export default SuspensionPrompt;
