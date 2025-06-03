"use client";

import { memo } from "react";

import { cn } from "@/libs/shadcn/utils";
import Markdown from "@/components/Markdown";

type Props = {
  content: string;
  className?: string;
};

function AnswerContent({ content, className }: Props) {
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <Markdown>{content}</Markdown>
    </div>
  );
}

export default memo(AnswerContent);
