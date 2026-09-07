import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import type { ExternalIdDraft } from "../utils";

/**
 * Rows of system × id: how this place is known to the tenant's other
 * systems (`toast: 1234`). The system is a lowercase token; the id is
 * whatever that system calls the place.
 */
export function ExternalIdsEditor({
  rows,
  onChange,
  errorSystem
}: {
  rows: ExternalIdDraft[];
  onChange: (rows: ExternalIdDraft[]) => void;
  /** The system whose id the server refused, to mark that row. */
  errorSystem?: string | null;
}) {
  const nextKey = rows.reduce((max, row) => Math.max(max, row.key), -1) + 1;
  const update = (key: number, patch: Partial<ExternalIdDraft>) =>
    onChange(rows.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  return (
    <div className='flex flex-col gap-2' data-testid='settings-locations-external-ids'>
      {rows.map((row, index) => {
        const invalid =
          errorSystem !== null && errorSystem !== undefined && row.system === errorSystem;
        return (
          <div key={row.key} className='flex items-center gap-2'>
            <Input
              placeholder='system'
              aria-label='System'
              autoComplete='off'
              spellCheck={false}
              value={row.system}
              onChange={(e) => update(row.key, { system: e.target.value.toLowerCase() })}
              className='w-32 font-mono text-xs'
              data-testid={`settings-locations-external-id-system-${index}`}
            />
            <Input
              placeholder='id in that system'
              aria-label='Id'
              autoComplete='off'
              value={row.id}
              onChange={(e) => update(row.key, { id: e.target.value })}
              aria-invalid={invalid ? true : undefined}
              className={`flex-1 font-mono text-xs ${
                invalid ? "border-destructive focus-visible:ring-destructive" : ""
              }`}
              data-testid={`settings-locations-external-id-value-${index}`}
            />
            <Button
              type='button'
              variant='ghost'
              size='icon'
              className='h-8 w-8 shrink-0 text-muted-foreground'
              onClick={() => onChange(rows.filter((r) => r.key !== row.key))}
              aria-label={`Remove ${row.system || "this"} id`}
              data-testid={`settings-locations-external-id-remove-${index}`}
            >
              <X className='h-4 w-4' />
            </Button>
          </div>
        );
      })}
      <Button
        type='button'
        variant='outline'
        size='sm'
        className='w-fit gap-1.5'
        onClick={() => onChange([...rows, { key: nextKey, system: "", id: "" }])}
        data-testid='settings-locations-external-id-add'
      >
        <Plus className='h-3.5 w-3.5' />
        Add external id
      </Button>
    </div>
  );
}
