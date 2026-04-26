import { ChevronDown, ChevronRight, Loader2, Search, Table2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { cn } from "@/libs/shadcn/utils";
import type { SchemaInfo } from "../types";

interface TableSelectorProps {
  schemas: SchemaInfo[];
  onConfirm: (selectedTables: string[]) => void;
  /**
   * Called when the user expands a schema that hasn't been loaded yet. The
   * orchestrator fetches its tables and populates `schema.tables`.
   */
  onExpandSchema?: (schema: string) => void;
  disabled?: boolean;
}

export default function TableSelector({
  schemas,
  onConfirm,
  onExpandSchema,
  disabled
}: TableSelectorProps) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  // Start with every schema collapsed. On warehouses with 200+ schemas,
  // auto-expanding all of them would both flood the UI and trigger a fetch
  // per schema. The user picks which schemas to explore.
  const [expandedSchemas, setExpandedSchemas] = useState<Set<string>>(() => new Set());
  const [filter, setFilter] = useState("");

  const filteredSchemas = useMemo(() => {
    if (!filter.trim()) return schemas;
    const lower = filter.toLowerCase();
    // Filter operates on already-loaded tables only. Schemas whose tables
    // haven't been fetched yet still appear (so the user can expand them to
    // trigger a fetch); schemas with zero matching tables drop out.
    return schemas
      .map((schema) => {
        if (!schema.loaded) return schema;
        return {
          ...schema,
          tables: schema.tables.filter(
            (t) =>
              t.name.toLowerCase().includes(lower) ||
              `${schema.schema}.${t.name}`.toLowerCase().includes(lower)
          )
        };
      })
      .filter((s) => !s.loaded || s.tables.length > 0);
  }, [schemas, filter]);

  const toggleSchema = useCallback(
    (schemaName: string, schema: SchemaInfo) => {
      setExpandedSchemas((prev) => {
        const next = new Set(prev);
        if (next.has(schemaName)) {
          next.delete(schemaName);
        } else {
          next.add(schemaName);
          // Kick off a lazy fetch the first time the schema is opened.
          if (!schema.loaded && !schema.loading) {
            onExpandSchema?.(schemaName);
          }
        }
        return next;
      });
    },
    [onExpandSchema]
  );

  const toggleTable = useCallback((fullName: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(fullName)) next.delete(fullName);
      else next.add(fullName);
      return next;
    });
  }, []);

  const toggleAllInSchema = useCallback(
    (schema: SchemaInfo) => {
      if (!schema.loaded || schema.tables.length === 0) return;
      const tableNames = schema.tables.map((t) => `${schema.schema}.${t.name}`);
      const allSelected = tableNames.every((n) => selected.has(n));
      setSelected((prev) => {
        const next = new Set(prev);
        for (const name of tableNames) {
          if (allSelected) next.delete(name);
          else next.add(name);
        }
        return next;
      });
    },
    [selected]
  );

  const handleConfirm = useCallback(() => {
    if (selected.size > 0) onConfirm(Array.from(selected));
  }, [selected, onConfirm]);

  return (
    <div className='flex flex-col gap-3'>
      {/* Search */}
      <div className='relative'>
        <Search className='absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground' />
        <Input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder='Search tables...'
          className='pl-9 text-sm'
          disabled={disabled}
        />
      </div>

      {/* Schema tree */}
      <div className='max-h-80 overflow-y-auto rounded-lg border border-border bg-card'>
        {filteredSchemas.length === 0 ? (
          <p className='p-4 text-center text-muted-foreground text-sm'>No schemas found</p>
        ) : (
          filteredSchemas.map((schema) => {
            const isExpanded = expandedSchemas.has(schema.schema);
            const schemaSelectable = schema.loaded && schema.tables.length > 0;
            const tableNames = schema.tables.map((t) => `${schema.schema}.${t.name}`);
            const selectedCount = tableNames.filter((n) => selected.has(n)).length;
            const allSelected = schemaSelectable && selectedCount === schema.tables.length;

            return (
              <div key={schema.schema}>
                {/* Schema header */}
                <div className='flex w-full items-center gap-2 px-3 py-2 hover:bg-muted/50'>
                  <button
                    type='button'
                    onClick={() => toggleSchema(schema.schema, schema)}
                    className='flex flex-1 items-center gap-2'
                  >
                    {isExpanded ? (
                      <ChevronDown className='h-3.5 w-3.5 text-muted-foreground' />
                    ) : (
                      <ChevronRight className='h-3.5 w-3.5 text-muted-foreground' />
                    )}
                    <span className='font-medium text-sm'>{schema.schema}</span>
                    <span className='text-muted-foreground text-xs'>
                      {schema.tableCount} table{schema.tableCount !== 1 ? "s" : ""}
                    </span>
                    {schema.loading && (
                      <Loader2 className='h-3 w-3 animate-spin text-muted-foreground' />
                    )}
                    {selectedCount > 0 && (
                      <span className='ml-auto text-primary text-xs'>{selectedCount} selected</span>
                    )}
                  </button>
                  {schemaSelectable && (
                    <button
                      type='button'
                      onClick={() => toggleAllInSchema(schema)}
                      className='text-muted-foreground text-xs hover:text-foreground'
                    >
                      {allSelected ? "Deselect all" : "Select all"}
                    </button>
                  )}
                </div>

                {/* Tables */}
                {isExpanded && (
                  <div className='pb-1'>
                    {schema.loading && !schema.loaded && (
                      <div className='flex items-center gap-2 px-3 py-1.5 pl-8 text-muted-foreground text-xs'>
                        <Loader2 className='h-3 w-3 animate-spin' />
                        Loading tables...
                      </div>
                    )}
                    {schema.loadError && (
                      <div className='px-3 py-1.5 pl-8 text-destructive text-xs'>
                        Failed to load tables: {schema.loadError}
                      </div>
                    )}
                    {schema.loaded && schema.tables.length === 0 && (
                      <div className='px-3 py-1.5 pl-8 text-muted-foreground text-xs'>
                        No tables in this schema.
                      </div>
                    )}
                    {schema.tables.map((table) => {
                      const fullName = `${schema.schema}.${table.name}`;
                      const isSelected = selected.has(fullName);
                      const columnCount = table.columnCount ?? table.columns.length;
                      return (
                        <button
                          key={fullName}
                          type='button'
                          onClick={() => toggleTable(fullName)}
                          className={cn(
                            "flex w-full items-center gap-2 px-3 py-1.5 pl-8 text-left transition-colors",
                            "hover:bg-muted/50",
                            isSelected && "bg-primary/5"
                          )}
                        >
                          <div
                            className={cn(
                              "flex h-4 w-4 items-center justify-center rounded border",
                              isSelected
                                ? "border-primary bg-primary text-primary-foreground"
                                : "border-muted-foreground/30"
                            )}
                          >
                            {isSelected && (
                              <svg
                                className='h-3 w-3'
                                fill='none'
                                viewBox='0 0 24 24'
                                stroke='currentColor'
                                strokeWidth={3}
                                role='img'
                                aria-label='Selected'
                              >
                                <title>Selected</title>
                                <path
                                  strokeLinecap='round'
                                  strokeLinejoin='round'
                                  d='M5 13l4 4L19 7'
                                />
                              </svg>
                            )}
                          </div>
                          <Table2 className='h-3.5 w-3.5 text-muted-foreground' />
                          <span className='text-sm'>{table.name}</span>
                          {columnCount > 0 && (
                            <span className='text-muted-foreground text-xs'>
                              {columnCount} col{columnCount !== 1 ? "s" : ""}
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Confirm */}
      <div className='flex items-center justify-between'>
        <span className='text-muted-foreground text-xs'>
          {selected.size} table{selected.size !== 1 ? "s" : ""} selected
        </span>
        <Button onClick={handleConfirm} disabled={disabled || selected.size === 0} size='sm'>
          Continue with {selected.size} table{selected.size !== 1 ? "s" : ""}
        </Button>
      </div>
    </div>
  );
}
