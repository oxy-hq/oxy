"use client";

import { memo } from "react";
import Markdown from "@/components/Markdown";
import { cn } from "@/libs/shadcn/utils";

type Props = {
  content: string;
  className?: string;
  onArtifactClick?: (id: string) => void;
};

function AnswerContent({ content, className, onArtifactClick }: Props) {
  return (
    <div className={cn("flex flex-col gap-4", className)} data-testid='agent-response-text'>
      <Markdown onArtifactClick={onArtifactClick}>{content}</Markdown>
    </div>
  );
}

export default memo(AnswerContent, (prevProps, nextProps) => {
  return prevProps.content === nextProps.content;
});
