import type React from "react";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import useApiKeys from "@/hooks/api/apiKeys/useApiKeys";
import TableContentWrapper from "../../components/TableContentWrapper";
import TableWrapper from "../../components/TableWrapper";
import ApiKeyRow from "./ApiKeyRow";

const ApiKeyTable: React.FC = () => {
  const { data: apiKeysData, isLoading, error, refetch } = useApiKeys();
  const apiKeys = apiKeysData?.api_keys || [];
  return (
    <TableWrapper>
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
          <TableContentWrapper
            isEmpty={apiKeys.length === 0}
            loading={isLoading}
            colSpan={5}
            error={error?.message}
            noFoundTitle='No API keys found'
            noFoundDescription='Create your first API key to get started.'
            onRetry={refetch}
          >
            {apiKeys.map((apiKey) => (
              <ApiKeyRow key={apiKey.id} apiKey={apiKey} />
            ))}
          </TableContentWrapper>
        </TableBody>
      </Table>
    </TableWrapper>
  );
};

export default ApiKeyTable;
