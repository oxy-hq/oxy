import { AlertCircle, CheckCircle, Clock, GitMerge } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { cn } from "@/libs/shadcn/utils";

interface SyncStatusBadgeProps {
  status: string;
  isInSync?: boolean;
}

export const SyncStatusBadge = ({ status, isInSync = false }: SyncStatusBadgeProps) => {
  const getStatusConfig = () => {
    switch (status.toLowerCase()) {
      case "synced":
        return {
          variant: "secondary" as const,
          icon: <CheckCircle className='h-3 w-3' />,
          label: "Synced",
          className: "bg-success/10 text-success border-success hover:bg-success/20"
        };
      case "syncing":
        return {
          variant: "outline" as const,
          icon: <Spinner className='size-3' />,
          label: "Syncing",
          className: "bg-info/10 text-info border-info/30 hover:bg-info/20"
        };
      case "behind":
        return {
          variant: "outline" as const,
          icon: <Clock className='h-3 w-3' />,
          label: "Behind",
          className: "bg-warning/10 text-warning border-warning/30 hover:bg-warning/20"
        };
      case "conflict":
        return {
          variant: "outline" as const,
          icon: <GitMerge className='h-3 w-3' />,
          label: "Merge conflict",
          className: "bg-warning/10 text-warning border-warning/30 hover:bg-warning/20"
        };
      case "error":
      case "failed":
        return {
          variant: "destructive" as const,
          icon: <AlertCircle className='h-3 w-3' />,
          label: "Error",
          className:
            "bg-destructive/10 text-destructive border-destructive/30 hover:bg-destructive/20"
        };
      default:
        return {
          variant: "outline" as const,
          icon: <Clock className='h-3 w-3' />,
          label: status.charAt(0).toUpperCase() + status.slice(1),
          className: ""
        };
    }
  };

  const config = getStatusConfig();

  if (isInSync && status.toLowerCase() === "synced") {
    config.label = "In Sync";
  }

  return (
    <Badge
      variant={config.variant}
      className={cn(
        "flex items-center gap-1.5 px-2 py-1 font-medium text-xs shadow-sm",
        config.className
      )}
    >
      {config.icon}
      {config.label}
    </Badge>
  );
};
