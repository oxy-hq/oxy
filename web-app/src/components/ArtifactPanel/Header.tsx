import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
} from "@/components/ui/shadcn/breadcrumb";
import { Artifact } from "@/services/mock";
import { Button } from "../ui/shadcn/button";
import { Download, X } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/shadcn/tooltip";
import { handleDownloadFile } from "@/libs/utils/string";

type Props = {
  artifactData: { [key: string]: Artifact };
  setSelectedArtifactIds: (ids: string[]) => void;
  selectedArtifactIds: string[];
  onClose: () => void;
  currentArtifact: Artifact;
};

const Header = ({
  artifactData,
  selectedArtifactIds,
  currentArtifact,
  setSelectedArtifactIds,
  onClose,
}: Props) => {
  const isSqlArtifact = currentArtifact.kind === "execute_sql";

  const handleDownloadSql = () => {
    if (!isSqlArtifact) {
      return;
    }
    const blob = new Blob([currentArtifact.content.value.sql_query], {
      type: "text/plain",
    });
    handleDownloadFile(blob, "query.sql");
  };

  return (
    <div className="w-fill flex px-4 py-2 align-center">
      <Breadcrumb className="flex-1 flex align-center">
        <BreadcrumbList>
          {selectedArtifactIds.map((artifact_id, index) => {
            const artifact = artifactData[artifact_id];
            return (
              artifact && (
                <div key={artifact.id}>
                  <BreadcrumbItem key={artifact.id}>
                    <BreadcrumbLink
                      onClick={() => {
                        setSelectedArtifactIds(
                          selectedArtifactIds.slice(0, index + 1),
                        );
                      }}
                    >
                      {artifact.name || `${artifact.kind} artifact`}
                    </BreadcrumbLink>
                  </BreadcrumbItem>
                  {index < selectedArtifactIds.length - 1 && (
                    <BreadcrumbSeparator />
                  )}
                </div>
              )
            );
          })}
        </BreadcrumbList>
      </Breadcrumb>
      <div className="flex gap-2">
        {isSqlArtifact && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                title="Download SQL"
                variant="outline"
                size="icon"
                onClick={handleDownloadSql}
              >
                <Download className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Download the SQL query</TooltipContent>
          </Tooltip>
        )}

        <Button variant="outline" size="icon" onClick={onClose}>
          <X />
        </Button>
      </div>
    </div>
  );
};

export default Header;
