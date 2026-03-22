import type React from "react";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import useSecrets from "@/hooks/api/secrets/useSecrets";
import TableContentWrapper from "../../components/TableContentWrapper";
import TableWrapper from "../../components/TableWrapper";
import { SecretRow } from "./Row";

export const SecretTable: React.FC = () => {
  const { data: secretsResponse, isLoading: loading, error, refetch } = useSecrets();
  const secrets = secretsResponse?.secrets || [];

  return (
    <TableWrapper>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Created</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableContentWrapper
            error={error?.message}
            isEmpty={secrets.length === 0}
            loading={loading}
            colSpan={3}
            noFoundTitle='No secrets'
            noFoundDescription='Create your first secret to securely store configuration values'
            onRetry={refetch}
          >
            {secrets.map((secret) => (
              <SecretRow key={secret.id} secret={secret} />
            ))}
          </TableContentWrapper>
        </TableBody>
      </Table>
    </TableWrapper>
  );
};
