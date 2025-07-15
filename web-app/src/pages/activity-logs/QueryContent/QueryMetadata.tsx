import { Badge } from "@/components/ui/shadcn/badge";
import { Bot, Database } from "lucide-react";

const QueryMetadata = ({
  queryItem,
}: {
  queryItem: Record<string, unknown>;
}) => {
  const database = queryItem.database as string | undefined;
  const isVerified = queryItem.is_verified as boolean | undefined;
  const source = queryItem.source ?? "";

  return (
    <div className="flex flex-wrap gap-2">
      {database && (
        <Badge>
          <Database className="h-4 w-4 mr-1" /> {database}
        </Badge>
      )}

      {source && (
        <Badge variant="outline">
          <Bot className="h-4 w-4 mr-1" /> {source.toString()}
        </Badge>
      )}

      {typeof isVerified === "boolean" && (
        <Badge variant={isVerified ? "default" : "destructive"}>
          {isVerified ? "✓ Verified" : "✗ Unverified"}
        </Badge>
      )}
    </div>
  );
};

export default QueryMetadata;
