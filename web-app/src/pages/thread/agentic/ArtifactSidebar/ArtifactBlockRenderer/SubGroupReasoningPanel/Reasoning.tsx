import {
  Blocks,
  Bot,
  ChevronDown,
  ChevronRight,
  CodeXml,
  GitBranch,
  Globe,
  Lightbulb,
  Save
} from "lucide-react";
import React, { type ReactNode, useState } from "react";
import type { Block, BlockBase, StepContent } from "@/services/types";
import AnswerContent from "../../../../../../components/AnswerContent";
import { BlockContent } from "../../../BlockMessage";

export type Step = StepContent &
  BlockBase & {
    childrenBlocks: Block[];
    routeGroupId?: string;
    routeName?: string;
  };

type ReasoningProps = {
  onFullscreen?: (blockId: string) => void;
  steps: Step[];
  header?: React.ReactNode;
};

const ReasoningItem = ({
  step,
  onFullscreen
}: {
  step: Step;
  onFullscreen?: (blockId: string) => void;
}) => {
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
  } else if (["save_automation"].includes(step.step_type)) {
    icon = <Save size={16} />;
  } else if (["semantic_query"].includes(step.step_type)) {
    icon = <Globe size={16} />;
  } else {
    icon = <CodeXml size={16} />;
  }

  return (
    <>
      <div className='w-full min-w-[500px]'>
        <div
          className='flex w-full cursor-pointer items-center justify-center gap-2 py-2'
          onClick={() => setIsExpanded(!isExpanded)}
        >
          {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <div className='flex flex-1 items-center justify-between text-sm'>
            <span className='flex items-center gap-1'>
              {icon}
              {step.step_type}
            </span>
            <span className='flex justify-end text-xs'>
              {(() => {
                if (step.is_streaming) {
                  return <span className='text-blue-800'>Processing</span>;
                }
                if (step.error) {
                  return <span className='text-red-800'>Error</span>;
                }
                return <span className='text-green-800'>Success</span>;
              })()}
            </span>
          </div>
        </div>
      </div>
      {isExpanded && (
        <div className='w-full min-w-[500px]' style={{ paddingLeft: 24 }}>
          {!!step.objective && <AnswerContent content={step.objective} />}
          {!!step.error && <span className='text-red-800'>{step.error}</span>}
          {step.childrenBlocks.map((child) => {
            return <BlockContent key={child.id} block={child} onFullscreen={onFullscreen} />;
          })}
        </div>
      )}
    </>
  );
};

const Reasoning = ({ steps, onFullscreen, header }: ReasoningProps) => {
  return (
    <div className='relative h-full w-full overflow-hidden'>
      <div className='customScrollbar absolute inset-0 flex h-full w-full flex-col overflow-auto p-4'>
        {header !== undefined ? (
          header
        ) : (
          <div className='mb-2 flex items-center justify-between'>
            {steps.length ? (
              <h2 className='text-sm'>Reasoning steps</h2>
            ) : (
              <h2 className='text-sm'>Thinking...</h2>
            )}
          </div>
        )}
        {steps.map((step) => (
          <ReasoningItem key={step.id} step={step} onFullscreen={onFullscreen} />
        ))}
      </div>
    </div>
  );
};

export default Reasoning;
