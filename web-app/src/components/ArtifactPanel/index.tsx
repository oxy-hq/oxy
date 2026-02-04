import { useQueries } from "@tanstack/react-query";
import { Loader2, X, XCircle } from "lucide-react";
import { useCallback } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ArtifactService } from "@/services/api";
import type { Artifact } from "@/types/artifact";
import { Alert, AlertDescription, AlertTitle } from "../ui/shadcn/alert";
import { Button } from "../ui/shadcn/button";
import AgentArtifactPanel from "./ArtifactsContent/agent";
import OmniQueryArtifactPanel from "./ArtifactsContent/omni-query";
import SandboxArtifactPanel from "./ArtifactsContent/sandbox-app";
import SemanticQueryArtifactPanel from "./ArtifactsContent/semantic-query";
import SqlArtifactPanel from "./ArtifactsContent/sql";
import WorkflowArtifactPanel from "./ArtifactsContent/workflow";
import Header from "./Header";

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
  setSelectedArtifactIds
}: Props) => {
  const onArtifactClick = useCallback(
    (id: string) => {
      setSelectedArtifactIds((prev) => [...prev, id]);
    },
    [setSelectedArtifactIds]
  );
  const { project, branchName } = useCurrentProjectBranch();
  const artifactQueries = useQueries({
    queries: selectedArtifactIds.map((id) => ({
      queryKey: ["artifact", project.id, branchName, id],
      queryFn: () => ArtifactService.getArtifact(project.id, branchName, id)
    }))
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
      {} as { [key: string]: Artifact }
    );
  const artifactData = {
    ...artifactStreamingData,
    ...artifactAPIData
  };

  const currentArtifactId = selectedArtifactIds[selectedArtifactIds.length - 1];
  const currentArtifact = artifactData[currentArtifactId];

  const isCurrentArtifactLoading =
    isLoading && !currentArtifact && !artifactStreamingData[currentArtifactId];

  if (isCurrentArtifactLoading) {
    return (
      <div className='flex h-full flex-col'>
        <div className='flex w-full justify-end px-4 py-2'>
          <Button variant='outline' size='icon' onClick={onClose}>
            <X />
          </Button>
        </div>

        <div className='flex flex-1 flex-col items-center justify-center space-y-4 text-gray-600'>
          <Loader2 className='animate-spin' />
          <p>Loading artifact...</p>
        </div>
      </div>
    );
  }

  if (hasError && !currentArtifact) {
    return (
      <div className='flex h-full w-full flex-col'>
        <div className='flex w-full justify-end px-4 py-2'>
          <Button variant='outline' size='icon' onClick={onClose}>
            <X />
          </Button>
        </div>

        <div className='flex flex-1 flex-col items-center justify-center gap-4 p-4'>
          <Alert variant='destructive'>
            <XCircle />
            <AlertTitle>Error</AlertTitle>
            <AlertDescription>
              Unable to load the selected artifact. Please check your connection or try again later.
            </AlertDescription>
          </Alert>
          <Button onClick={() => artifactQueries.forEach((query) => query.refetch())}>Retry</Button>
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

    if (currentArtifact.kind === "omni_query") {
      return <OmniQueryArtifactPanel artifact={currentArtifact} />;
    }

    if (currentArtifact.kind === "agent") {
      return (
        <div className='customScrollbar h-full overflow-y-auto p-4'>
          <AgentArtifactPanel artifact={currentArtifact} onArtifactClick={onArtifactClick} />
        </div>
      );
    }

    if (currentArtifact.kind === "workflow") {
      return <WorkflowArtifactPanel onArtifactClick={onArtifactClick} artifact={currentArtifact} />;
    }

    if (currentArtifact.kind === "sandbox_app") {
      return <SandboxArtifactPanel artifact={currentArtifact} />;
    }

    return <div className='artifact-unknown'>Unsupported artifact type</div>;
  };

  const handleClose = () => {
    setSelectedArtifactIds([]);
    onClose();
  };

  return (
    <div className='flex h-full flex-col'>
      <Header
        currentArtifact={currentArtifact}
        artifactData={artifactData}
        setSelectedArtifactIds={setSelectedArtifactIds}
        selectedArtifactIds={selectedArtifactIds}
        onClose={handleClose}
      />
      <div className='min-h-0 flex-1'>{renderContent()}</div>
    </div>
  );
};

export default ArtifactPanel;
