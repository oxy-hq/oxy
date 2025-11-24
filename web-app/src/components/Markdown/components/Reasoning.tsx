import React, { useState } from "react";
import { ChevronDown } from "lucide-react";

type Props = {
  children: React.ReactNode;
  isLoading?: boolean;
};

const ReasoningContainer = React.memo(function ReasoningContainer({
  children,
}: Props) {
  const [isExpanded, setIsExpanded] = useState(true);

  return (
    <div className="my-4">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
        type="button"
      >
        <ChevronDown
          className={`h-4 w-4 transition-transform ${
            isExpanded ? "rotate-180" : ""
          }`}
        />
        <span className="flex items-center gap-2">Reasoning...</span>
      </button>
      {isExpanded && (
        <div className="mt-2 ml-6 text-sm text-muted-foreground leading-relaxed border-l-2 border-muted pl-3">
          {children}
        </div>
      )}
    </div>
  );
});

export default ReasoningContainer;
