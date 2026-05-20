"use client";

import { memo } from "react";
import { ErrorBoundary } from "react-error-boundary";
import Markdown from "@/components/Markdown";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { cn } from "@/libs/shadcn/utils";

type Props = {
  content: string;
  className?: string;
  onArtifactClick?: (id: string) => void;
};

function AnswerContent({ content, className, onArtifactClick }: Props) {
  return (
    <div className={cn("flex flex-col gap-4", className)} data-testid='agent-response-text'>
      <ErrorBoundary
        resetKeys={[content]}
        fallback={
          <ErrorAlert
            title='Failed to render message'
            message='The message content could not be displayed.'
          />
        }
      >
        <Markdown onArtifactClick={onArtifactClick}>{content}</Markdown>
      </ErrorBoundary>
    </div>
  );
}

export default memo(AnswerContent, (prevProps, nextProps) => {
  return prevProps.content === nextProps.content;
});
