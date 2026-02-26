import { useState } from "react";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import ExpandedDetails from "./ExpandedDetails";
import StepHeader from "./StepHeader";

interface ReasoningStepRowProps {
  step: Step;
  dagNodeId: string | null;
  hoveredNodeId: string | null;
  onStepHover: (dagNodeId: string | null) => void;
  showDagMapping?: boolean;
  onArtifactClick?: (blockId: string) => void;
}

const ReasoningStepRow = ({
  step,
  dagNodeId,
  hoveredNodeId,
  onStepHover,
  showDagMapping = false,
  onArtifactClick
}: ReasoningStepRowProps) => {
  const [expanded, setExpanded] = useState(false);

  return (
    <div>
      <StepHeader
        step={step}
        dagNodeId={dagNodeId}
        hoveredNodeId={hoveredNodeId}
        expanded={expanded}
        onToggle={() => setExpanded((v) => !v)}
        onStepHover={onStepHover}
        onArtifactClick={onArtifactClick}
      />

      <ExpandedDetails
        step={step}
        expanded={expanded}
        dagNodeId={dagNodeId}
        showDagMapping={showDagMapping}
        onArtifactClick={onArtifactClick}
      />
    </div>
  );
};

export default ReasoningStepRow;
