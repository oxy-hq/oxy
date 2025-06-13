import { AgentArtifact } from "@/services/mock";
import AnswerContent from "@/components/AnswerContent";

type Props = {
  artifact: AgentArtifact;
  onArtifactClick?: (id: string) => void;
};

const AgentArtifactPanel = ({ artifact, onArtifactClick }: Props) => {
  return (
    <AnswerContent
      content={artifact.content.value.output}
      onArtifactClick={onArtifactClick}
    />
  );
};

export default AgentArtifactPanel;
