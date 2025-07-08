import React from "react";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
  TableCell,
} from "@/components/ui/shadcn/table";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Edit, Trash2 } from "lucide-react";
import { Secret } from "@/types/secret";
import { formatDistanceToNow } from "date-fns";
import { useMediaQuery } from "usehooks-ts";

interface SecretTableProps {
  secrets: Secret[];
  loading: boolean;
  onEditClick: (secret: Secret) => void;
  onDeleteClick: (secret: Secret) => void;
}

export const SecretTable: React.FC<SecretTableProps> = ({
  secrets,
  loading,
  onEditClick,
  onDeleteClick,
}) => {
  const isMobile = useMediaQuery("(max-width: 767px)");

  const renderTableContent = () => {
    const colSpan = isMobile ? 3 : 6; // Adjust based on visible columns

    if (loading) {
      return (
        <TableRow>
          <TableCell colSpan={colSpan} className="text-center py-8">
            <div className="flex items-center justify-center">
              <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-gray-900"></div>
              <span className="ml-2">Loading secrets...</span>
            </div>
          </TableCell>
        </TableRow>
      );
    }

    if (secrets.length === 0) {
      return (
        <TableRow>
          <TableCell colSpan={colSpan} className="text-center py-8">
            <div className="text-muted-foreground">
              <p className="text-lg mb-2">No secrets found</p>
              <p className="text-sm">
                Create your first secret to securely store configuration values
              </p>
            </div>
          </TableCell>
        </TableRow>
      );
    }

    return secrets.map((secret) => (
      <TableRow key={secret.id}>
        <TableCell className="font-medium">
          <div className="flex flex-col gap-1">
            <span>{secret.name}</span>
            {isMobile && secret.description && (
              <span className="text-xs text-muted-foreground mt-1">
                {secret.description}
              </span>
            )}
          </div>
        </TableCell>
        {!isMobile && (
          <TableCell>
            {secret.description || (
              <span className="text-muted-foreground italic">
                No description
              </span>
            )}
          </TableCell>
        )}
        <TableCell>
          <Badge variant={secret.is_active ? "default" : "secondary"}>
            {secret.is_active ? "Active" : "Inactive"}
          </Badge>
        </TableCell>
        {!isMobile && (
          <TableCell>
            {formatDistanceToNow(new Date(secret.created_at), {
              addSuffix: true,
            })}
          </TableCell>
        )}
        {!isMobile && (
          <TableCell>
            {formatDistanceToNow(new Date(secret.updated_at), {
              addSuffix: true,
            })}
          </TableCell>
        )}
        <TableCell>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onEditClick(secret)}
              className="h-8 w-8 p-0"
              title="Edit secret"
            >
              <Edit className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onDeleteClick(secret)}
              className="h-8 w-8 p-0 text-destructive hover:text-destructive"
              title="Delete secret"
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        </TableCell>
      </TableRow>
    ));
  };

  return (
    <div className="border rounded-lg overflow-hidden">
      <div className="overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              {!isMobile && <TableHead>Description</TableHead>}
              <TableHead>Status</TableHead>
              {!isMobile && <TableHead>Created</TableHead>}
              {!isMobile && <TableHead>Updated</TableHead>}
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>{renderTableContent()}</TableBody>
        </Table>
      </div>
    </div>
  );
};
