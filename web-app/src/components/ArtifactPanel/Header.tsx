import { Download } from "lucide-react";
import { PanelHeader } from "@/components/ui/panel";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator
} from "@/components/ui/shadcn/breadcrumb";
import { Button } from "@/components/ui/shadcn/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { handleDownloadFile } from "@/libs/utils/string";
import type { Artifact } from "@/types/artifact";

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
  onClose
}: Props) => {
  const isSqlArtifact =
    currentArtifact.kind === "execute_sql" || currentArtifact.kind === "semantic_query";

  const handleDownloadSql = () => {
    if (!isSqlArtifact) {
      return;
    }
    const blob = new Blob([currentArtifact.content.value.sql_query], {
      type: "text/plain"
    });
    handleDownloadFile(blob, "query.sql");
  };

  const breadcrumb = (
    <Breadcrumb>
      <BreadcrumbList>
        {selectedArtifactIds.map((artifact_id, index) => {
          const artifact = artifactData[artifact_id];
          return (
            artifact && (
              <div className='flex items-center gap-2' key={artifact.id}>
                <BreadcrumbItem>
                  <BreadcrumbLink
                    className='cursor-pointer'
                    onClick={() => {
                      setSelectedArtifactIds(selectedArtifactIds.slice(0, index + 1));
                    }}
                  >
                    {artifact.name || `${artifact.kind} artifact`}
                  </BreadcrumbLink>
                </BreadcrumbItem>
                {index < selectedArtifactIds.length - 1 && <BreadcrumbSeparator />}
              </div>
            )
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );

  const downloadAction = isSqlArtifact ? (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          title='Download SQL'
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          onClick={handleDownloadSql}
        >
          <Download className='h-4 w-4' />
        </Button>
      </TooltipTrigger>
      <TooltipContent>Download the SQL query</TooltipContent>
    </Tooltip>
  ) : undefined;

  return <PanelHeader title={breadcrumb} actions={downloadAction} onClose={onClose} />;
};

export default Header;
