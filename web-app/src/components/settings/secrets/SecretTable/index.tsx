import React from "react";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { useMediaQuery } from "usehooks-ts";
import { SecretRow } from "./Row";
import TableContentWrapper from "../../components/TableContentWrapper";
import TableWrapper from "../../components/TableWrapper";
import useSecrets from "@/hooks/api/useSecrets";

export const SecretTable: React.FC = () => {
  const {
    data: secretsResponse,
    isLoading: loading,
    error,
    refetch,
  } = useSecrets();
  const secrets = secretsResponse?.secrets || [];
  const isMobile = useMediaQuery("(max-width: 767px)");

  return (
    <TableWrapper>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            {!isMobile && <TableHead>Description</TableHead>}
            <TableHead>Status</TableHead>
            {!isMobile && <TableHead>Created</TableHead>}
            {!isMobile && <TableHead>Updated</TableHead>}
            <TableHead className="text-center">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableContentWrapper
            error={error?.message}
            isEmpty={secrets.length === 0}
            loading={loading}
            colSpan={isMobile ? 3 : 6}
            noFoundTitle="No secrets found"
            noFoundDescription="Create your first secret to securely store configuration values"
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
