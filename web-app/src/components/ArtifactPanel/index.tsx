import SqlArtifactPanel from "./ArtifactsContent/sql";
import AgentArtifactPanel from "./ArtifactsContent/agent";
import WorkflowArtifactPanel from "./ArtifactsContent/workflow";
import { useQueries } from "@tanstack/react-query";
import { service } from "@/services/service";
import Header from "./Header";
import { Artifact } from "@/services/mock";
import { useCallback } from "react";

type Props = {
  selectedArtifactIds: string[];
  artifactStreamingData: { [key: string]: Artifact };
  onClose: () => void;
  setSelectedArtifactIds: React.Dispatch<React.SetStateAction<string[]>>;
};

const ArtifactPanel = ({
  selectedArtifactIds,
  artifactStreamingData,
  onClose,
  setSelectedArtifactIds,
}: Props) => {
  const onArtifactClick = useCallback(
    (id: string) => {
      setSelectedArtifactIds((prev) => [...prev, id]);
    },
    [setSelectedArtifactIds],
  );
  const artifactQueries = useQueries({
    queries: selectedArtifactIds.map((id) => ({
      queryKey: ["artifact", id],
      queryFn: () => service.getArtifact(id),
    })),
  });
  const artifactAPIData: { [key: string]: Artifact } = artifactQueries
    .map((result) => result.data)
    .reduce(
      (acc, artifact) => {
        if (artifact) {
          acc[artifact.id] = artifact;
        }
        return acc;
      },
      {} as { [key: string]: Artifact },
    );
  const artifactData = {
    ...artifactStreamingData,
    ...artifactAPIData,
  };

  const currentArtifact =
    artifactData[selectedArtifactIds[selectedArtifactIds.length - 1]];

  if (!currentArtifact) {
    return null;
  }
  const renderContent = () => {
    if (currentArtifact.kind === "execute_sql") {
      return <SqlArtifactPanel artifact={currentArtifact} />;
    }

    if (currentArtifact.kind === "agent") {
      return (
        <div className="h-full p-4 overflow-y-auto customScrollbar">
          <AgentArtifactPanel
            artifact={currentArtifact}
            onArtifactClick={onArtifactClick}
          />
        </div>
      );
    }

    if (currentArtifact.kind === "workflow") {
      return (
        <WorkflowArtifactPanel
          onArtifactClick={onArtifactClick}
          artifact={currentArtifact}
        />
      );
    }

    return <div className="artifact-unknown">Unsupported artifact type</div>;
  };

  const handleClose = () => {
    setSelectedArtifactIds([]);
    onClose();
  };

  return (
    <div className="h-full ">
      <Header
        currentArtifact={currentArtifact}
        artifactData={artifactData}
        setSelectedArtifactIds={setSelectedArtifactIds}
        selectedArtifactIds={selectedArtifactIds}
        onClose={handleClose}
      />
      <div className="h-full">{renderContent()}</div>
    </div>
  );
};

export default ArtifactPanel;
