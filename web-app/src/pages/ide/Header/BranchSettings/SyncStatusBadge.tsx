import { AlertCircle, CheckCircle, Clock, Loader2 } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
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
          className:
            "bg-green-100 text-green-800 border-green-200 hover:bg-green-200 dark:bg-green-900/20 dark:text-green-400 dark:border-green-800"
        };
      case "syncing":
        return {
          variant: "outline" as const,
          icon: <Loader2 className='h-3 w-3 animate-spin' />,
          label: "Syncing",
          className:
            "bg-blue-100 text-blue-800 border-blue-200 hover:bg-blue-200 dark:bg-blue-900/20 dark:text-blue-400 dark:border-blue-800"
        };
      case "behind":
        return {
          variant: "outline" as const,
          icon: <Clock className='h-3 w-3' />,
          label: "Behind",
          className:
            "bg-yellow-100 text-yellow-800 border-yellow-200 hover:bg-yellow-200 dark:bg-yellow-900/20 dark:text-yellow-400 dark:border-yellow-800"
        };
      case "error":
      case "failed":
        return {
          variant: "destructive" as const,
          icon: <AlertCircle className='h-3 w-3' />,
          label: "Error",
          className:
            "bg-red-100 text-red-800 border-red-200 hover:bg-red-200 dark:bg-red-900/20 dark:text-red-400 dark:border-red-800"
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
