import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { WarehousesFormData } from ".";
import { useFormContext } from "react-hook-form";

export default function DuckDBForm({ index }: { index: number }) {
  const {
    formState: { errors },
    register,
  } = useFormContext<WarehousesFormData>();

  const fieldErrors = errors?.warehouses?.[index]?.config as
    | Record<string, { message?: string }>
    | undefined;

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`dataset-${index}`}>Dataset</Label>
        <Input
          id={`dataset-${index}`}
          placeholder="/path/to/your/data"
          {...register(`warehouses.${index}.config.dataset`, {
            required: "Dataset is required",
          })}
        />
        {fieldErrors?.dataset && (
          <p className="text-xs text-destructive mt-1">
            {fieldErrors.dataset.message?.toString()}
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          Enter the path to your DuckDB database file
        </p>
      </div>
    </div>
  );
}
