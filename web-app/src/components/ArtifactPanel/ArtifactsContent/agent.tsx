import AnswerContent from "@/components/AnswerContent";
import type { AgentArtifact } from "@/types/artifact";

type Props = {
  artifact: AgentArtifact;
  onArtifactClick?: (id: string) => void;
};

const AgentArtifactPanel = ({ artifact, onArtifactClick }: Props) => {
  return (
    <AnswerContent content={artifact.content.value.output} onArtifactClick={onArtifactClick} />
  );
};

export default AgentArtifactPanel;
