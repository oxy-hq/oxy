import { formatDistanceToNow } from "date-fns";
import {
  Check,
  ClipboardCopy,
  Code2,
  Edit,
  Eye,
  EyeOff,
  Plus,
  Trash2,
  UserRound
} from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { CreateSecretDialog } from "@/components/settings/secrets/CreateSecretDialog";
import { DeleteSecretDialog } from "@/components/settings/secrets/SecretTable/Row/DeleteSecretDialog";
import { EditSecretDialog } from "@/components/settings/secrets/SecretTable/Row/EditSecretDialog";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import useEnvSecrets from "@/hooks/api/secrets/useEnvSecrets";
import { useDeleteSecret } from "@/hooks/api/secrets/useSecretMutations";
import useSecrets from "@/hooks/api/secrets/useSecrets";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { SecretService } from "@/services/secretService";
import type { EnvSecret, Secret } from "@/types/secret";
import TableContentWrapper from "../../components/TableContentWrapper";
import TableWrapper from "../../components/TableWrapper";

interface UnifiedRow {
  key: string;
  name: string;
  source: "secret" | "dot_env" | "environment" | "not_set";
  referencedBy?: string | null;
  maskedValue?: string;
  envFullValue?: string; // always available for env rows (admin-only endpoint)
  secretInfo?: Secret;
  envInfo?: EnvSecret;
}

function buildRows(secrets: Secret[], envSecrets: EnvSecret[]): UnifiedRow[] {
  const envMap = new Map<string, EnvSecret>();
  for (const e of envSecrets) {
    envMap.set(e.env_var, e);
  }

  const secretMap = new Map<string, Secret>();
  for (const s of secrets) {
    secretMap.set(s.name, s);
  }

  const rows: UnifiedRow[] = [];
  const seen = new Set<string>();

  // DB secrets first
  for (const secret of secrets) {
    const env = envMap.get(secret.name);
    // DB secret always shows as "Secret" — the env backing is shown via "overrides X" text
    rows.push({
      key: `secret-${secret.id}`,
      name: secret.name,
      source: "secret",
      referencedBy: env?.referenced_by,
      maskedValue: env?.masked_value, // env masked value (env source)
      envFullValue: env?.full_value,
      secretInfo: secret,
      envInfo: env
    });
    seen.add(secret.name);
  }

  // Env vars not overridden by a DB secret (include unset ones so users know what's missing)
  for (const env of envSecrets) {
    if (seen.has(env.env_var)) continue;
    rows.push({
      key: `env-${env.env_var}-${env.referenced_by ?? ""}`,
      name: env.env_var,
      source: env.source,
      referencedBy: env.referenced_by,
      maskedValue: env.masked_value,
      envFullValue: env.full_value,
      envInfo: env
    });
  }

  rows.sort((a, b) => a.name.localeCompare(b.name));
  return rows;
}

const SOURCE_CONFIG = {
  secret: { label: "Secret", className: "border-blue-500/30 bg-blue-500/10 text-blue-400" },
  dot_env: { label: ".env", className: "border-amber-500/30 bg-amber-500/10 text-amber-400" },
  environment: { label: "env", className: "border-green-500/30 bg-green-500/10 text-green-400" },
  not_set: { label: "not set", className: "border-red-500/30 bg-red-500/10 text-red-400" }
} as const;

const DOTS = "••••••••••••••";

export const UnifiedSecretsTable: React.FC = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const {
    data: secretsResponse,
    isLoading: secretsLoading,
    error: secretsError,
    refetch: refetchSecrets
  } = useSecrets();
  const {
    data: envSecrets = [],
    isLoading: envLoading,
    error: envError,
    refetch: refetchEnv
  } = useEnvSecrets();

  const deleteSecretMutation = useDeleteSecret();

  const [revealedValues, setRevealedValues] = useState<Record<string, string>>({});
  const [revealLoading, setRevealLoading] = useState<Record<string, boolean>>({});
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [createDialogName, setCreateDialogName] = useState<string | undefined>();
  const [editSecret, setEditSecret] = useState<Secret | null>(null);
  const [deleteSecret, setDeleteSecret] = useState<Secret | null>(null);

  const secrets = secretsResponse?.secrets ?? [];
  const isLoading = secretsLoading || envLoading;
  const error = secretsError || envError;
  const rows = buildRows(secrets, envSecrets);

  const handleReveal = async (row: UnifiedRow) => {
    const key = row.key;

    if (revealedValues[key] !== undefined) {
      // Hide
      setRevealedValues((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
      return;
    }

    if (row.secretInfo) {
      // Reveal from DB
      setRevealLoading((prev) => ({ ...prev, [key]: true }));
      try {
        const value = await SecretService.revealSecret(projectId, row.secretInfo.id);
        setRevealedValues((prev) => ({ ...prev, [key]: value }));
      } catch {
        toast.error("Failed to reveal secret value");
      } finally {
        setRevealLoading((prev) => ({ ...prev, [key]: false }));
      }
    } else if (row.envFullValue !== undefined) {
      // Env vars: full value already available from API
      setRevealedValues((prev) => ({ ...prev, [key]: row.envFullValue ?? "" }));
    }
  };

  const handleCopy = async (row: UnifiedRow) => {
    let value = revealedValues[row.key];
    if (value === undefined) {
      if (row.secretInfo) {
        try {
          value = await SecretService.revealSecret(projectId, row.secretInfo.id);
        } catch {
          toast.error("Failed to copy secret value");
          return;
        }
      } else {
        value = row.envFullValue ?? "";
      }
    }
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      toast.error("Failed to copy to clipboard");
      return;
    }
    setCopiedKey(row.key);
    setTimeout(() => setCopiedKey((k) => (k === row.key ? null : k)), 1500);
  };

  const handleDelete = async () => {
    if (!deleteSecret) return;
    await deleteSecretMutation.mutateAsync(deleteSecret.id);
    setDeleteSecret(null);
  };

  const handleRefetch = () => {
    refetchSecrets();
    refetchEnv();
  };

  const isRevealed = (key: string) => revealedValues[key] !== undefined;

  return (
    <>
      <TableWrapper>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className='w-1/3'>Variable</TableHead>
              <TableHead className='w-1/4'>Value</TableHead>
              <TableHead>Source</TableHead>
              <TableHead>Updated</TableHead>
              <TableHead className='w-px whitespace-nowrap'>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableContentWrapper
              isEmpty={rows.length === 0}
              loading={isLoading}
              colSpan={5}
              error={error?.message}
              noFoundTitle='No secrets configured'
              noFoundDescription='Create a secret or add environment variables to get started'
              onRetry={handleRefetch}
            >
              {rows.map((row) => {
                const revealed = isRevealed(row.key);
                const displayValue = revealed ? revealedValues[row.key] : DOTS;
                const sourceConfig = SOURCE_CONFIG[row.source];
                const date = row.secretInfo?.updated_at;
                const isUnset = !row.secretInfo && !row.envInfo?.is_set;

                return (
                  <TableRow key={row.key} className='group'>
                    <TableCell className='w-1/3 max-w-0'>
                      <div className='flex min-w-0 items-center gap-2'>
                        <Code2 className='size-3.5 shrink-0 text-muted-foreground/50' />
                        <span className='truncate font-medium font-mono text-sm'>{row.name}</span>
                      </div>
                      {row.secretInfo?.description && (
                        <p className='mt-0.5 truncate pl-[22px] font-mono text-muted-foreground text-xs'>
                          {row.secretInfo.description}
                        </p>
                      )}
                    </TableCell>

                    <TableCell className='w-1/4 max-w-0 overflow-hidden'>
                      {isUnset ? (
                        <Badge
                          variant='outline'
                          className='border-red-500/30 bg-red-500/10 font-medium text-[10px] text-red-400'
                        >
                          Not set
                        </Badge>
                      ) : (
                        <span
                          className={
                            revealed
                              ? "block min-w-0 truncate font-mono text-sm tabular-nums"
                              : "block truncate font-mono text-muted-foreground/40 text-sm tracking-widest"
                          }
                        >
                          {displayValue}
                        </span>
                      )}
                    </TableCell>

                    <TableCell>
                      <div className='flex flex-col gap-1'>
                        <Badge
                          variant='outline'
                          className={`w-fit font-medium text-[10px] ${sourceConfig.className}`}
                        >
                          {sourceConfig.label}
                        </Badge>
                        {row.secretInfo && row.referencedBy && (
                          <span className='text-[10px] text-muted-foreground/50'>
                            overrides {row.referencedBy}
                          </span>
                        )}
                        {row.referencedBy && !row.secretInfo && (
                          <span className='text-[10px] text-muted-foreground/50'>
                            {row.referencedBy}
                          </span>
                        )}
                      </div>
                    </TableCell>

                    <TableCell className='whitespace-nowrap text-muted-foreground text-sm'>
                      {date ? (
                        <div className='space-y-0.5'>
                          <div>{formatDistanceToNow(new Date(date), { addSuffix: true })}</div>
                          {(row.secretInfo?.updated_by_email ??
                            row.secretInfo?.created_by_email) && (
                            <div className='flex items-center gap-1 text-muted-foreground/60 text-xs'>
                              <UserRound className='size-3 shrink-0' />
                              <span className='max-w-[140px] truncate'>
                                {row.secretInfo?.updated_by_email ??
                                  row.secretInfo?.created_by_email}
                              </span>
                            </div>
                          )}
                        </div>
                      ) : (
                        "—"
                      )}
                    </TableCell>

                    <TableCell className='w-px whitespace-nowrap'>
                      <div className='flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100'>
                        {/* Reveal toggle — hidden for unset env vars */}
                        {!isUnset && (
                          <Button
                            variant='ghost'
                            size='sm'
                            className='size-7 p-0'
                            onClick={() => handleReveal(row)}
                            disabled={revealLoading[row.key]}
                            title={revealed ? "Hide value" : "Reveal value"}
                          >
                            {revealed ? (
                              <EyeOff className='size-3.5' />
                            ) : (
                              <Eye className='size-3.5' />
                            )}
                          </Button>
                        )}

                        {/* Copy value — hidden for unset env vars */}
                        {!isUnset && (
                          <Button
                            variant='ghost'
                            size='sm'
                            className='size-7 p-0'
                            onClick={() => handleCopy(row)}
                            title='Copy value'
                          >
                            {copiedKey === row.key ? (
                              <Check className='size-3.5 text-green-400' />
                            ) : (
                              <ClipboardCopy className='size-3.5' />
                            )}
                          </Button>
                        )}

                        {/* Edit — DB secrets only */}
                        {row.secretInfo && (
                          <Button
                            variant='ghost'
                            size='sm'
                            className='size-7 p-0'
                            onClick={() => row.secretInfo && setEditSecret(row.secretInfo)}
                            title='Edit secret'
                          >
                            <Edit className='size-3.5' />
                          </Button>
                        )}

                        {/* Delete — DB secrets only */}
                        {row.secretInfo && (
                          <Button
                            variant='ghost'
                            size='sm'
                            className='size-7 p-0'
                            onClick={() => row.secretInfo && setDeleteSecret(row.secretInfo)}
                            title='Delete secret'
                          >
                            <Trash2 className='!text-destructive size-3.5' />
                          </Button>
                        )}

                        {/* Add override — env-only rows */}
                        {!row.secretInfo && (
                          <Button
                            variant='ghost'
                            size='sm'
                            className='size-7 p-0'
                            onClick={() => setCreateDialogName(row.name)}
                            title='Add secret override'
                          >
                            <Plus className='size-3.5' />
                          </Button>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableContentWrapper>
          </TableBody>
        </Table>
      </TableWrapper>

      <CreateSecretDialog
        open={createDialogName !== undefined}
        onOpenChange={(open) => !open && setCreateDialogName(undefined)}
        initialName={createDialogName}
        onSecretCreated={() => {
          toast.success("Secret created successfully");
          setCreateDialogName(undefined);
        }}
      />

      {editSecret && (
        <EditSecretDialog
          open
          onOpenChange={(open) => !open && setEditSecret(null)}
          secret={editSecret}
          onSecretUpdated={() => {
            toast.success("Secret updated successfully");
            setEditSecret(null);
          }}
        />
      )}

      {deleteSecret && (
        <DeleteSecretDialog
          open
          onOpenChange={(open) => !open && setDeleteSecret(null)}
          secret={deleteSecret}
          onConfirm={handleDelete}
        />
      )}
    </>
  );
};
