import { CircleCheck, Clock, MessageCircleQuestion } from "lucide-react";
import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

export const AskUserView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    prompt?: string;
    suggestions?: string[];
  }>(item.toolInput);
  const output = parseToolJson<{ answer?: string } | string>(item.toolOutput);

  const prompt = input?.prompt ?? "";
  const suggestions = input?.suggestions ?? [];
  const answer = typeof output === "string" ? output : output?.answer;
  const answered = !item.isStreaming;

  return (
    <div className='flex h-full min-h-0 flex-col p-4'>
      <div className='flex min-h-0 flex-1 flex-col gap-4'>
        <div className='rounded border bg-muted/30 px-2.5 py-2'>
          <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Status</p>
          <div className='mt-0.5 flex items-center gap-1.5'>
            {answered ? (
              <>
                <CircleCheck className='h-3.5 w-3.5 text-emerald-500' />
                <p className='font-medium text-xs'>Answered</p>
              </>
            ) : (
              <>
                <Clock className='h-3.5 w-3.5 text-amber-500' />
                <p className='font-medium text-xs'>Awaiting response</p>
              </>
            )}
          </div>
        </div>

        {prompt && (
          <div>
            <p className='mb-1.5 flex items-center gap-1.5 font-medium text-muted-foreground text-xs'>
              <MessageCircleQuestion className='h-3.5 w-3.5' />
              Question
            </p>
            <div className='rounded border bg-muted/20 px-3 py-2'>
              <p className='whitespace-pre-wrap text-xs'>{prompt}</p>
            </div>
          </div>
        )}

        {suggestions.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Suggestions</p>
            <div className='flex flex-wrap gap-1.5'>
              {suggestions.map((suggestion) => (
                <span
                  key={suggestion}
                  className={`rounded-full border px-2.5 py-0.5 text-xs ${
                    answered && answer === suggestion
                      ? "border-primary/40 bg-primary/10 font-medium text-primary"
                      : "bg-muted/30 text-muted-foreground"
                  }`}
                >
                  {suggestion}
                </span>
              ))}
            </div>
          </div>
        )}

        {answered && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Answer</p>
            <div className='rounded border border-primary/20 bg-primary/5 px-3 py-2'>
              <p className='whitespace-pre-wrap text-xs'>{answer}</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
