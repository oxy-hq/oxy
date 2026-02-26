import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import CollapsibleSection from "./CollapsibleSection";

export interface FieldItem {
  value: string;
  label: string;
}

interface FieldListProps {
  title: string;
  fields: string[];
  availableItems?: FieldItem[];
  placeholder?: string;
  searchPlaceholder?: string;
  addLabel?: string;
  editable?: boolean;
  onFieldChange?: (index: number, value: string) => void;
  onFieldRemove?: (index: number) => void;
  onFieldAdd?: () => void;
}

const FieldList = ({
  title,
  fields,
  availableItems = [],
  placeholder = "Select...",
  searchPlaceholder = "Search...",
  addLabel = "Add field",
  editable = false,
  onFieldChange,
  onFieldRemove,
  onFieldAdd
}: FieldListProps) => {
  if (fields.length === 0 && !editable) return null;

  return (
    <CollapsibleSection title={title} count={fields.length}>
      <div className='flex flex-col gap-2'>
        {" "}
        {/* Using index in key is necessary here since field values can be duplicated/reordered */}{" "}
        {fields.map((field, i) => (
          <div key={`${title}-${field}-${i}`} className='flex items-center gap-1.5'>
            {editable && availableItems.length > 0 ? (
              <>
                <div className='min-w-0 flex-1'>
                  <Combobox
                    items={availableItems}
                    value={field}
                    onValueChange={(v) => onFieldChange?.(i, v)}
                    placeholder={placeholder}
                    searchPlaceholder={searchPlaceholder}
                    className='h-8 text-xs'
                  />
                </div>
                <Button
                  variant='ghost'
                  size='icon'
                  className='h-7 w-7 shrink-0'
                  onClick={() => onFieldRemove?.(i)}
                >
                  <X className='h-3 w-3' />
                </Button>
              </>
            ) : (
              <span className='rounded-md bg-muted px-2 py-1 font-mono text-xs'>
                {field.split(".").pop()}
              </span>
            )}
          </div>
        ))}
        {editable && onFieldAdd && (
          <Button
            variant='ghost'
            size='sm'
            className='h-7 w-fit text-muted-foreground text-xs'
            onClick={onFieldAdd}
          >
            <Plus className='mr-1 h-3 w-3' />
            {addLabel}
          </Button>
        )}
      </div>
    </CollapsibleSection>
  );
};

export default FieldList;
