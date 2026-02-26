import SemanticQueryPanel from "@/components/SemanticQueryPanel";
import type { SemanticQueryArtifact } from "@/types/artifact";

type Props = {
  artifact: SemanticQueryArtifact;
};

const SemanticQueryArtifactPanel = ({ artifact }: Props) => {
  return <SemanticQueryPanel artifact={artifact} sqlDefaultOpen={true} showDatabase={true} />;
};

export default SemanticQueryArtifactPanel;
