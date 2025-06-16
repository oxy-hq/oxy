import React from "react";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { ApiKey } from "@/types/apiKey";
import { ApiKeyTableContent } from "./ApiKeyTableContent";

interface ApiKeyTableProps {
  apiKeys: ApiKey[];
  loading: boolean;
  onDeleteClick: (apiKey: ApiKey) => void;
}

export const ApiKeyTable: React.FC<ApiKeyTableProps> = ({
  apiKeys,
  loading,
  onDeleteClick,
}) => {
  return (
    <div className="border rounded-lg">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Last Used</TableHead>
            <TableHead>Created</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <ApiKeyTableContent
            apiKeys={apiKeys}
            loading={loading}
            onDeleteClick={onDeleteClick}
          />
        </TableBody>
      </Table>
    </div>
  );
};
