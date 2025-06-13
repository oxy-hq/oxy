"use client";

import { memo } from "react";

import { cn } from "@/libs/shadcn/utils";
import Markdown from "@/components/Markdown";

type Props = {
  content: string;
  className?: string;
  onArtifactClick?: (id: string) => void;
};

function AnswerContent({ content, className, onArtifactClick }: Props) {
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <Markdown onArtifactClick={onArtifactClick}>{content}</Markdown>
    </div>
  );
}

export default memo(AnswerContent);
