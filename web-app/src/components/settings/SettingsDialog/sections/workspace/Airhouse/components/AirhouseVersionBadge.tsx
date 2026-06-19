import { Badge } from "@/components/ui/shadcn/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import useAirhouseVersion from "@/hooks/api/airhouse/useAirhouseVersion";

/**
 * Small badge showing the running Airhouse deployment's software version,
 * mirroring Oxy's own VersionBadge. The version is global to the deployment,
 * so it renders regardless of whether this workspace is provisioned yet.
 *
 * Hidden entirely while loading, on error, or when Airhouse isn't configured
 * (503) — it's ancillary info and should never render a broken or
 * "unavailable" state next to the connection panel.
 */
export const AirhouseVersionBadge = () => {
  const { data } = useAirhouseVersion();

  if (!data?.version) {
    return null;
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant='secondary' className='font-mono font-normal'>
          v{data.version}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>Airhouse server version</TooltipContent>
    </Tooltip>
  );
};
