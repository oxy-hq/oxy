import SqlArtifactPanel from "./ArtifactsContent/sql";
import SemanticQueryArtifactPanel from "./ArtifactsContent/semantic-query";
import AgentArtifactPanel from "./ArtifactsContent/agent";
import WorkflowArtifactPanel from "./ArtifactsContent/workflow";
import { useQueries } from "@tanstack/react-query";
import { ArtifactService } from "@/services/api";
import Header from "./Header";
import { useCallback } from "react";
import { Button } from "../ui/shadcn/button";
import { Alert, AlertDescription, AlertTitle } from "../ui/shadcn/alert";
import { Loader2, X, XCircle } from "lucide-react";
import { Artifact } from "@/types/artifact";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

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
  const { project, branchName } = useCurrentProjectBranch();
  const artifactQueries = useQueries({
    queries: selectedArtifactIds.map((id) => ({
      queryKey: ["artifact", project.id, branchName, id],
      queryFn: () => ArtifactService.getArtifact(project.id, branchName, id),
    })),
  });

  const isLoading = artifactQueries.some((query) => query.isLoading);
  const hasError = artifactQueries.some((query) => query.isError);

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

  const currentArtifactId = selectedArtifactIds[selectedArtifactIds.length - 1];
  const currentArtifact = artifactData[currentArtifactId];

  const isCurrentArtifactLoading =
    isLoading && !currentArtifact && !artifactStreamingData[currentArtifactId];

  if (isCurrentArtifactLoading) {
    return (
      <div className="h-full flex flex-col">
        <div className="w-full flex px-4 py-2 justify-end">
          <Button variant="outline" size="icon" onClick={onClose}>
            <X />
          </Button>
        </div>

        <div className="flex-1 flex flex-col items-center justify-center space-y-4 text-gray-600">
          <Loader2 className="animate-spin" />
          <p>Loading artifact...</p>
        </div>
      </div>
    );
  }

  if (hasError && !currentArtifact) {
    return (
      <div className="flex h-full w-full flex-col">
        <div className="w-full flex px-4 py-2 justify-end">
          <Button variant="outline" size="icon" onClick={onClose}>
            <X />
          </Button>
        </div>

        <div className="p-4 gap-4 flex-1 flex-col flex items-center justify-center">
          <Alert variant="destructive">
            <XCircle />
            <AlertTitle>Error</AlertTitle>
            <AlertDescription>
              Unable to load the selected artifact. Please check your connection
              or try again later.
            </AlertDescription>
          </Alert>
          <Button
            onClick={() => artifactQueries.forEach((query) => query.refetch())}
          >
            Retry
          </Button>
        </div>
      </div>
    );
  }

  if (!currentArtifact) {
    return null;
  }

  const renderContent = () => {
    if (currentArtifact.kind === "execute_sql") {
      return <SqlArtifactPanel artifact={currentArtifact} />;
    }

    if (currentArtifact.kind === "semantic_query") {
      return <SemanticQueryArtifactPanel artifact={currentArtifact} />;
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
    <div className="h-full flex flex-col">
      <Header
        currentArtifact={currentArtifact}
        artifactData={artifactData}
        setSelectedArtifactIds={setSelectedArtifactIds}
        selectedArtifactIds={selectedArtifactIds}
        onClose={handleClose}
      />
      <div className="flex-1 min-h-0">{renderContent()}</div>
    </div>
  );
};

export default ArtifactPanel;
