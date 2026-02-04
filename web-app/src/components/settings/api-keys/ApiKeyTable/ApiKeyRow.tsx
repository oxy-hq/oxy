import { Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { useRevokeApiKey } from "@/hooks/api/apiKeys/useApiKeyMutations";
import { ApiKeyService } from "@/services/api/apiKey";
import type { ApiKey } from "@/types/apiKey";
import DeleteApiKeyDialog from "./DeleteApiKeyDialog";

interface Props {
  apiKey: ApiKey;
}

const ApiKeyRow: React.FC<Props> = ({ apiKey }) => {
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);

  const revokeApiKeyMutation = useRevokeApiKey();

  const handleDeleteApiKey = async () => {
    await revokeApiKeyMutation.mutateAsync(apiKey.id);
    setIsDeleteDialogOpen(false);
  };

  const openDeleteDialog = () => {
    setIsDeleteDialogOpen(true);
  };

  const formatLastUsed = () => {
    const lastUsedAt = apiKey.last_used_at;
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

  const getStatusBadge = () => {
    if (!apiKey.is_active) {
      return <Badge variant='destructive'>Revoked</Badge>;
    }

    if (ApiKeyService.isExpired(apiKey.expires_at)) {
      return <Badge variant='destructive'>Expired</Badge>;
    }

    const timeUntilExpiration = ApiKeyService.getTimeUntilExpiration(apiKey.expires_at);
    if (timeUntilExpiration === null) {
      return <Badge variant='default'>Active</Badge>;
    }

    return (
      <div className='flex items-center gap-2'>
        <Badge variant='default'>Active</Badge>
        <span className='text-muted-foreground text-sm'>Expires in {timeUntilExpiration}</span>
      </div>
    );
  };

  return (
    <TableRow key={apiKey.id}>
      <TableCell>
        <div>
          <div className='font-medium'>{apiKey.name}</div>
          {apiKey.masked_key && (
            <div className='font-mono text-muted-foreground text-sm'>{apiKey.masked_key}</div>
          )}
        </div>
      </TableCell>
      <TableCell>{getStatusBadge()}</TableCell>
      <TableCell>{formatLastUsed()}</TableCell>
      <TableCell>{ApiKeyService.formatDate(apiKey.created_at)}</TableCell>
      <TableCell>
        <Button variant='ghost' size='sm' onClick={openDeleteDialog} disabled={!apiKey.is_active}>
          <Trash2 className='!text-destructive' />
        </Button>
      </TableCell>
      <DeleteApiKeyDialog
        open={isDeleteDialogOpen}
        onOpenChange={setIsDeleteDialogOpen}
        apiKey={apiKey}
        onConfirm={handleDeleteApiKey}
      />
    </TableRow>
  );
};

export default ApiKeyRow;
