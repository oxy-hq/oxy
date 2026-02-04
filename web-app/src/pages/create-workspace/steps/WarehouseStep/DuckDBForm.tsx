import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { WarehousesFormData } from ".";

export default function DuckDBForm({ index }: { index: number }) {
  const {
    formState: { errors },
    register
  } = useFormContext<WarehousesFormData>();

  const fieldErrors = errors?.warehouses?.[index]?.config as
    | Record<string, { message?: string }>
    | undefined;

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`dataset-${index}`}>Dataset</Label>
        <Input
          id={`dataset-${index}`}
          placeholder='/path/to/your/data'
          {...register(`warehouses.${index}.config.dataset`, {
            required: "Dataset is required"
          })}
        />
        {fieldErrors?.dataset && (
          <p className='mt-1 text-destructive text-xs'>{fieldErrors.dataset.message?.toString()}</p>
        )}
        <p className='text-muted-foreground text-xs'>Enter the path to your DuckDB database file</p>
      </div>
    </div>
  );
}
