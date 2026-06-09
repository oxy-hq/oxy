import type React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Switch } from "@/components/ui/shadcn/switch";
import type { WriteDisposition } from "../../../scaffold";
import type { useClickHouseCredentials } from "./useClickHouseCredentials";

const DISPOSITIONS: { value: WriteDisposition; label: string; hint: string }[] = [
  {
    value: "append",
    label: "Append",
    hint: "insert rows (rerun duplicates unless a cursor is set)"
  },
  { value: "replace", label: "Replace", hint: "overwrite the whole table each run" },
  { value: "merge", label: "Merge", hint: "upsert on a key (idempotent reruns)" }
];

interface ClickHouseCredentialsFormProps {
  credentials: ReturnType<typeof useClickHouseCredentials>;
  onClearError: () => void;
  onError: (message: string | null) => void;
}

const ClickHouseCredentialsForm: React.FC<ClickHouseCredentialsFormProps> = ({
  credentials,
  onClearError,
  onError
}) => (
  <div className='grid gap-3 rounded-md border border-border p-3'>
    <p className='font-medium text-sm'>ClickHouse connection</p>
    <div className='grid grid-cols-[1fr_auto] gap-2'>
      <div className='grid gap-2'>
        <Label htmlFor='ch-host'>Host</Label>
        <Input
          id='ch-host'
          value={credentials.host}
          onChange={(e) => {
            credentials.setHost(e.target.value);
            onClearError();
          }}
          placeholder='my-host.clickhouse.cloud'
        />
      </div>
      <div className='grid w-24 gap-2'>
        <Label htmlFor='ch-port'>Port</Label>
        <Input
          id='ch-port'
          value={credentials.port}
          onChange={(e) => credentials.setPort(e.target.value)}
          placeholder='8443'
          inputMode='numeric'
        />
      </div>
    </div>
    <div className='grid grid-cols-2 gap-2'>
      <div className='grid gap-2'>
        <Label htmlFor='ch-database'>Database</Label>
        <Input
          id='ch-database'
          value={credentials.database}
          onChange={(e) => {
            credentials.setDatabase(e.target.value);
            onClearError();
          }}
          placeholder='default'
        />
      </div>
      <div className='grid gap-2'>
        <Label htmlFor='ch-username'>Username</Label>
        <Input
          id='ch-username'
          value={credentials.username}
          onChange={(e) => credentials.setUsername(e.target.value)}
          placeholder='default'
        />
      </div>
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='ch-password'>Password</Label>
      <Input
        id='ch-password'
        type='password'
        value={credentials.password}
        onChange={(e) => credentials.setPassword(e.target.value)}
        placeholder='Stored as a secret — needed to list tables'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='ch-secret-name'>Secret name</Label>
      <Input
        id='ch-secret-name'
        value={credentials.secretName}
        onChange={(e) => {
          credentials.setSecretName(e.target.value);
          onClearError();
        }}
        placeholder='CLICKHOUSE_PASSWORD'
      />
      <p className='text-muted-foreground text-xs'>
        The pipeline references this name; the executor resolves it from the secret manager at run
        time.
      </p>
    </div>
    <div className='flex items-center justify-between'>
      <Label htmlFor='ch-secure'>Use TLS (HTTPS)</Label>
      <Switch id='ch-secure' checked={credentials.secure} onCheckedChange={credentials.setSecure} />
    </div>

    <div className='flex items-center justify-between gap-2'>
      <Button
        type='button'
        variant='secondary'
        size='sm'
        onClick={() => credentials.fetchTables(onError)}
        disabled={
          credentials.discovering ||
          !credentials.host.trim() ||
          !credentials.database.trim() ||
          !credentials.password
        }
      >
        {credentials.discovering ? "Listing tables…" : "Fetch tables"}
      </Button>
      {credentials.tables != null && credentials.tables.length > 0 && (
        <div className='flex items-center gap-2'>
          <span className='text-muted-foreground text-xs'>
            {credentials.selectedCount} of {credentials.tables.length} selected
          </span>
          <Button type='button' variant='ghost' size='sm' onClick={credentials.toggleSelectAll}>
            {credentials.allTablesSelected ? "Clear" : "Select all"}
          </Button>
        </div>
      )}
    </div>

    {credentials.tables != null &&
      (credentials.tables.length === 0 ? (
        <p className='text-muted-foreground text-xs'>
          No tables found in “{credentials.database.trim()}”.
        </p>
      ) : (
        <div className='max-h-56 overflow-y-auto rounded-md border border-border'>
          {credentials.tables.map((t) => {
            const sel = credentials.selection[t.name];
            return (
              <div key={t.name} className='border-border border-b last:border-0'>
                <label
                  htmlFor={`ch-table-${t.name}`}
                  className='flex cursor-pointer items-center gap-2 px-2 py-1.5 text-sm hover:bg-muted/50'
                >
                  <Checkbox
                    id={`ch-table-${t.name}`}
                    checked={sel != null}
                    onCheckedChange={() => credentials.toggleTable(t.name)}
                  />
                  <span className='truncate'>{t.name}</span>
                </label>
                {sel != null && (
                  <div className='flex flex-wrap items-center gap-2 px-2 pb-2 pl-8'>
                    <Select
                      value={sel.disposition}
                      onValueChange={(v) =>
                        credentials.setTableConfig(t.name, { disposition: v as WriteDisposition })
                      }
                    >
                      <SelectTrigger className='h-7 w-28 text-xs'>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {DISPOSITIONS.map((d) => (
                          <SelectItem key={d.value} value={d.value} title={d.hint}>
                            {d.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>

                    {sel.disposition === "append" && (
                      <Select
                        value={sel.cursorField ?? "__none"}
                        onValueChange={(v) =>
                          credentials.setTableConfig(t.name, {
                            cursorField: v === "__none" ? undefined : v
                          })
                        }
                      >
                        <SelectTrigger className='h-7 w-44 text-xs'>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value='__none'>No cursor (full reload)</SelectItem>
                          {t.columns.map((c) => (
                            <SelectItem key={c.name} value={c.name}>
                              {c.name}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}

                    {sel.disposition === "merge" && (
                      <Select
                        value={sel.primaryKey ?? ""}
                        onValueChange={(v) => credentials.setTableConfig(t.name, { primaryKey: v })}
                      >
                        <SelectTrigger className='h-7 w-44 text-xs'>
                          <SelectValue placeholder='merge key (required)' />
                        </SelectTrigger>
                        <SelectContent>
                          {t.columns.map((c) => (
                            <SelectItem key={c.name} value={c.name}>
                              {c.name}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ))}
  </div>
);

export default ClickHouseCredentialsForm;
