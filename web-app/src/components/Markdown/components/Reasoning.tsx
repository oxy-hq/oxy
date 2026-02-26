import { ChevronDown } from "lucide-react";
import React, { useState } from "react";

type Props = {
  children: React.ReactNode;
  isLoading?: boolean;
};

const ReasoningContainer = React.memo(function ReasoningContainer({ children }: Props) {
  const [isExpanded, setIsExpanded] = useState(true);

  return (
    <div className='mb-4'>
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className='flex items-center gap-2 text-muted-foreground text-sm transition-colors hover:text-foreground'
        type='button'
      >
        <ChevronDown className={`h-4 w-4 transition-transform ${isExpanded ? "rotate-180" : ""}`} />
        <span className='flex items-center gap-2'>Reasoning...</span>
      </button>
      {isExpanded && (
        <div className='mt-2 ml-6 border-muted border-l-2 pl-3 text-muted-foreground text-sm leading-relaxed'>
          {children}
        </div>
      )}
    </div>
  );
});

export default ReasoningContainer;
