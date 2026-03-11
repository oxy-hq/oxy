import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import type { FieldPath } from "react-hook-form";
import { Controller, useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { AppFormData } from "../index";
import { TaskColumnSelect, TaskRefSelect } from "./components";

interface RowDisplayFieldsProps {
  index: number;
}

// Display types allowed inside a row (no nested rows)
const CHILD_DISPLAY_TYPES = [
  { value: "table", label: "Table" },
  { value: "bar_chart", label: "Bar Chart" },
  { value: "line_chart", label: "Line Chart" },
  { value: "pie_chart", label: "Pie Chart" },
  { value: "markdown", label: "Markdown" }
];

// Helper to cast nested template-literal paths TypeScript can't statically resolve.
const fp = (path: string) => path as FieldPath<AppFormData>;

interface RowChildFieldsProps {
  parentIndex: number;
  childIndex: number;
}

const RowChildFields: React.FC<RowChildFieldsProps> = ({ parentIndex, childIndex }) => {
  const { register, control, watch } = useFormContext<AppFormData>();
  const childType = watch(fp(`display.${parentIndex}.children.${childIndex}.type`)) as
    | string
    | undefined;
  const dataSource = watch(fp(`display.${parentIndex}.children.${childIndex}.data`)) as
    | string
    | undefined;

  const path = (field: string) => fp(`display.${parentIndex}.children.${childIndex}.${field}`);

  switch (childType) {
    case "markdown":
      return (
        <div className='space-y-2'>
          <Label>Content *</Label>
          <Textarea
            placeholder='Enter markdown content'
            rows={5}
            {...register(path("content"), { required: "Content is required" })}
          />
        </div>
      );

    case "table":
      return (
        <div className='space-y-4'>
          <div className='space-y-2'>
            <Label>Data Source *</Label>
            <Controller
              control={control}
              name={path("data")}
              rules={{ required: "Data source is required" }}
              render={({ field }) => (
                <TaskRefSelect
                  value={field.value as string | undefined}
                  onChange={field.onChange}
                  placeholder='Select task...'
                />
              )}
            />
          </div>
          <div className='space-y-2'>
            <Label>Title</Label>
            <Input placeholder='Table title' {...register(path("title"))} />
          </div>
        </div>
      );

    case "bar_chart":
    case "line_chart":
      return (
        <div className='space-y-4'>
          <div className='space-y-2'>
            <Label>Data Source *</Label>
            <Controller
              control={control}
              name={path("data")}
              rules={{ required: "Data source is required" }}
              render={({ field }) => (
                <TaskRefSelect
                  value={field.value as string | undefined}
                  onChange={field.onChange}
                  placeholder='Select task...'
                />
              )}
            />
          </div>
          <div className='space-y-2'>
            <Label>Title</Label>
            <Input placeholder='Chart title' {...register(path("title"))} />
          </div>
          <div className='grid grid-cols-2 gap-4'>
            <div className='space-y-2'>
              <Label>X Axis *</Label>
              <Controller
                control={control}
                name={path("x")}
                rules={{ required: "X axis is required" }}
                render={({ field }) => (
                  <TaskColumnSelect
                    taskName={dataSource}
                    value={field.value as string | undefined}
                    onChange={field.onChange}
                    placeholder='Column name'
                  />
                )}
              />
            </div>
            <div className='space-y-2'>
              <Label>Y Axis *</Label>
              <Controller
                control={control}
                name={path("y")}
                rules={{ required: "Y axis is required" }}
                render={({ field }) => (
                  <TaskColumnSelect
                    taskName={dataSource}
                    value={field.value as string | undefined}
                    onChange={field.onChange}
                    placeholder='Column name'
                  />
                )}
              />
            </div>
          </div>
        </div>
      );

    case "pie_chart":
      return (
        <div className='space-y-4'>
          <div className='space-y-2'>
            <Label>Data Source *</Label>
            <Controller
              control={control}
              name={path("data")}
              rules={{ required: "Data source is required" }}
              render={({ field }) => (
                <TaskRefSelect
                  value={field.value as string | undefined}
                  onChange={field.onChange}
                  placeholder='Select task...'
                />
              )}
            />
          </div>
          <div className='space-y-2'>
            <Label>Title</Label>
            <Input placeholder='Chart title' {...register(path("title"))} />
          </div>
          <div className='grid grid-cols-2 gap-4'>
            <div className='space-y-2'>
              <Label>Name (label column) *</Label>
              <Controller
                control={control}
                name={path("name")}
                rules={{ required: "Name column is required" }}
                render={({ field }) => (
                  <TaskColumnSelect
                    taskName={dataSource}
                    value={field.value as string | undefined}
                    onChange={field.onChange}
                    placeholder='Column name'
                  />
                )}
              />
            </div>
            <div className='space-y-2'>
              <Label>Value (numeric column) *</Label>
              <Controller
                control={control}
                name={path("value")}
                rules={{ required: "Value column is required" }}
                render={({ field }) => (
                  <TaskColumnSelect
                    taskName={dataSource}
                    value={field.value as string | undefined}
                    onChange={field.onChange}
                    placeholder='Column name'
                  />
                )}
              />
            </div>
          </div>
        </div>
      );

    default:
      return null;
  }
};

interface RowChildFormProps {
  parentIndex: number;
  childIndex: number;
  onRemove: () => void;
  /** Open the collapsible on first mount (used for newly appended children). */
  defaultOpen?: boolean;
}

const RowChildForm: React.FC<RowChildFormProps> = ({
  parentIndex,
  childIndex,
  onRemove,
  defaultOpen = false
}) => {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const { control, watch, setValue } = useFormContext<AppFormData>();
  const childType = watch(fp(`display.${parentIndex}.children.${childIndex}.type`)) as
    | string
    | undefined;

  const getLabel = (type: string | undefined) =>
    CHILD_DISPLAY_TYPES.find((t) => t.value === type)?.label || type || "Display";

  return (
    <div className='rounded-md border p-3'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className='w-full rounded-lg transition-colors'>
          <div className='flex items-center justify-between'>
            {isOpen ? (
              <ChevronDown className='h-4 w-4 text-muted-foreground' />
            ) : (
              <ChevronRight className='h-4 w-4 text-muted-foreground' />
            )}
            <div className='flex flex-1 items-center gap-2 px-2'>
              <span className='font-medium text-sm'>Column {childIndex + 1}</span>
              {childType && (
                <span className='rounded-md bg-muted px-2 py-0.5 text-muted-foreground text-xs'>
                  {getLabel(childType)}
                </span>
              )}
            </div>
            <Button
              type='button'
              onClick={(e) => {
                e.stopPropagation();
                onRemove();
              }}
              variant='ghost'
              size='sm'
            >
              <Trash2 className='h-4 w-4' />
            </Button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent className='mt-3 space-y-3'>
          <div className='space-y-2'>
            <Label>Type *</Label>
            <Controller
              name={fp(`display.${parentIndex}.children.${childIndex}.type`)}
              control={control}
              rules={{ required: "Type is required" }}
              render={({ field }) => (
                <Select
                  onValueChange={(value) => {
                    setValue(fp(`display.${parentIndex}.children.${childIndex}`), { type: value });
                    field.onChange(value);
                  }}
                  value={field.value as string | undefined}
                >
                  <SelectTrigger>
                    <SelectValue placeholder='Select type' />
                  </SelectTrigger>
                  <SelectContent>
                    {CHILD_DISPLAY_TYPES.map((t) => (
                      <SelectItem className='cursor-pointer' key={t.value} value={t.value}>
                        {t.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            />
          </div>
          <RowChildFields parentIndex={parentIndex} childIndex={childIndex} />
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};

export const RowDisplayFields: React.FC<RowDisplayFieldsProps> = ({ index }) => {
  const {
    register,
    control,
    formState: { errors }
  } = useFormContext<AppFormData>();

  const { fields, append, remove } = useFieldArray({
    control,
    name: fp(`display.${index}.children`) as "display"
  });

  // Children that existed at mount start collapsed; newly appended ones open
  // immediately so the user sees required fields without an extra click.
  const [initialCount] = useState(fields.length);

  const handleAppend = () => append({ type: "table" });

  // errors.display is typed as an array of FieldErrors for DisplayFormData.
  // Columns lives on the parent display item, not a child, so we reach into
  // the array by index. The cast avoids relying on the loose [key:string]:unknown
  // index signature which would give us `unknown` for .columns.message.
  const columnsError = (errors.display?.[index] as { columns?: { message?: string } } | undefined)
    ?.columns;
  const childrenError = (errors.display?.[index] as { children?: { message?: string } } | undefined)
    ?.children;

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`display.${index}.columns`}>Columns</Label>
        <Input
          id={`display.${index}.columns`}
          type='number'
          min={1}
          max={6}
          placeholder='Auto (matches child count)'
          {...register(fp(`display.${index}.columns`), {
            setValueAs: (v: string) => (v === "" ? undefined : Number(v)),
            min: { value: 1, message: "Minimum 1 column" },
            max: { value: 6, message: "Maximum 6 columns" }
          })}
        />
        {columnsError && (
          <p className='text-red-500 text-sm'>{String(columnsError.message || "")}</p>
        )}
        <p className='text-muted-foreground text-sm'>
          Number of equal-width columns; defaults to number of children
        </p>
      </div>

      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <Label>Children</Label>
          <Button type='button' variant='outline' size='sm' onClick={handleAppend}>
            <Plus className='mr-1 h-3 w-3' />
            Add Column
          </Button>
        </div>
        {/* Hidden controller purely for RHF validation — blocks submit when empty. */}
        <Controller
          control={control}
          name={fp(`display.${index}.children`)}
          rules={{
            validate: (v) => (Array.isArray(v) && v.length > 0) || "Add at least one column"
          }}
          render={() => <></>}
        />
        {childrenError && (
          <p className='text-red-500 text-sm'>{String(childrenError.message || "")}</p>
        )}
        <div className='space-y-2'>
          {fields.map((field, childIndex) => (
            <RowChildForm
              key={field.id}
              parentIndex={index}
              childIndex={childIndex}
              onRemove={() => remove(childIndex)}
              defaultOpen={childIndex >= initialCount}
            />
          ))}
          {fields.length === 0 && (
            <p className='py-4 text-center text-muted-foreground text-sm'>
              No columns yet. Add at least one.
            </p>
          )}
        </div>
      </div>
    </div>
  );
};
