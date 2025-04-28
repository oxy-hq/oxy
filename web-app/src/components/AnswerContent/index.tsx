"use client";

import { memo } from "react";

import CodeContainer from "./Code";
import { cn } from "@/libs/shadcn/utils";
import ChartPlugin from "./ChartPlugin";
import ChartContainer from "./Chart";
import Markdown from "../Markdown";
import { ExtendedComponents } from "react-markdown";

type Props = {
  content: string;
  className?: string;
};

const extendedComponents: ExtendedComponents = {
  code: (props) => <CodeContainer {...props} />,
  chart: (props) => <ChartContainer {...props} />,
};

function AnswerContent({ content, className }: Props) {
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <Markdown plugins={[ChartPlugin]} components={extendedComponents}>
        {content}
      </Markdown>
    </div>
  );
}

export default memo(AnswerContent);
