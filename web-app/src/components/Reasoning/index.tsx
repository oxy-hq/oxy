import { BlockBase, StepContent } from "@/services/types";
import {
  Blocks,
  Bot,
  ChevronDown,
  ChevronRight,
  CodeXml,
  GitBranch,
  Lightbulb,
} from "lucide-react";
import { ReactNode, useState } from "react";
import AnswerContent from "../AnswerContent";

type ChildBlock = {
  id: string;
  content: ReactNode[];
};

export type Step = StepContent &
  BlockBase & {
    childrenBlocks: ChildBlock[];
  };

type ReasoningProps = {
  steps: Step[];
};

const ReasoningItem = ({ step }: { step: Step }) => {
  const [isExpanded, setIsExpanded] = useState(false);
  let icon: ReactNode;
  if (["idle", "plan", "end"].includes(step.step_type)) {
    icon = <Bot size={16} />;
  } else if (["build_app"].includes(step.step_type)) {
    icon = <Blocks size={16} />;
  } else if (["insight"].includes(step.step_type)) {
    icon = <Lightbulb size={16} />;
  } else if (["subflow"].includes(step.step_type)) {
    icon = <GitBranch size={16} />;
  } else {
    icon = <CodeXml size={16} />;
  }

  return (
    <>
      <div className="w-full min-w-[500px]">
        <div
          className="w-full flex items-center justify-center py-2 gap-2 cursor-pointer"
          onClick={() => setIsExpanded(!isExpanded)}
        >
          {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <div className="flex-1 text-sm flex justify-between items-center">
            <span className="flex items-center gap-1">
              {icon}
              {step.step_type}
            </span>
            <span className="text-xs flex justify-end">
              {(() => {
                if (step.is_streaming) {
                  return <span className="text-blue-800">Processing</span>;
                }
                if (step.error) {
                  return <span className="text-red-800">Error</span>;
                }
                return <span className="text-green-800">Success</span>;
              })()}
            </span>
          </div>
        </div>
      </div>
      {isExpanded && (
        <div className="w-full min-w-[500px]" style={{ paddingLeft: 24 }}>
          {!!step.objective && <AnswerContent content={step.objective} />}
          {step.childrenBlocks.map((child) => child.content)}
        </div>
      )}
    </>
  );
};

const Reasoning = ({ steps }: ReasoningProps) => {
  return (
    <div className="h-full w-full overflow-hidden relative">
      <div className="absolute customScrollbar flex flex-col h-full inset-0 overflow-auto p-4 w-full">
        <div className="flex justify-between items-center mb-2">
          <h2 className="text-sm">Reasoning steps</h2>
        </div>
        {steps.map((step) => (
          <ReasoningItem key={step.id} step={step} />
        ))}
      </div>
    </div>
  );
};

export default Reasoning;
