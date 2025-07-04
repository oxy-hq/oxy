import React from "react";
import { Trash2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { Badge } from "@/components/ui/shadcn/badge";
import { ApiKeyService } from "@/services/api/apiKey";
import { ApiKey } from "@/types/apiKey";

interface ApiKeyTableContentProps {
  apiKeys: ApiKey[];
  loading: boolean;
  onDeleteClick: (apiKey: ApiKey) => void;
}

export const ApiKeyTableContent: React.FC<ApiKeyTableContentProps> = ({
  apiKeys,
  loading,
  onDeleteClick,
}) => {
  const formatLastUsed = (lastUsedAt?: string) => {
    if (!lastUsedAt) return "Never";
    const date = new Date(lastUsedAt);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) return "Today";
    if (diffDays === 1) return "Yesterday";
    if (diffDays < 7) return `${diffDays} days ago`;

    return ApiKeyService.formatDate(lastUsedAt);
  };

  const getStatusBadge = (apiKey: ApiKey) => {
    if (!apiKey.is_active) {
      return <Badge variant="destructive">Revoked</Badge>;
    }

    if (ApiKeyService.isExpired(apiKey.expires_at)) {
      return <Badge variant="destructive">Expired</Badge>;
    }

    const timeUntilExpiration = ApiKeyService.getTimeUntilExpiration(
      apiKey.expires_at,
    );
    if (timeUntilExpiration === null) {
      return <Badge variant="default">Active</Badge>;
    }

    return (
      <div className="flex items-center gap-2">
        <Badge variant="default">Active</Badge>
        <span className="text-sm text-muted-foreground">
          Expires in {timeUntilExpiration}
        </span>
      </div>
    );
  };

  if (loading) {
    return (
      <TableRow>
        <TableCell colSpan={5} className="text-center py-8">
          Loading API keys...
        </TableCell>
      </TableRow>
    );
  }

  if (apiKeys.length === 0) {
    return (
      <TableRow>
        <TableCell colSpan={5} className="text-center py-8">
          <div className="text-muted-foreground">
            <p>No API keys found</p>
            <p className="text-sm mt-1">
              Create your first API key to get started
            </p>
          </div>
        </TableCell>
      </TableRow>
    );
  }

  return (
    <>
      {apiKeys.map((apiKey) => (
        <TableRow key={apiKey.id}>
          <TableCell>
            <div>
              <div className="font-medium">{apiKey.name}</div>
              {apiKey.masked_key && (
                <div className="text-sm text-muted-foreground font-mono">
                  {apiKey.masked_key}
                </div>
              )}
            </div>
          </TableCell>
          <TableCell>{getStatusBadge(apiKey)}</TableCell>
          <TableCell>{formatLastUsed(apiKey.last_used_at)}</TableCell>
          <TableCell>{ApiKeyService.formatDate(apiKey.created_at)}</TableCell>
          <TableCell>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onDeleteClick(apiKey)}
              disabled={!apiKey.is_active}
            >
              <Trash2 className="w-4 h-4" />
            </Button>
          </TableCell>
        </TableRow>
      ))}
    </>
  );
};
