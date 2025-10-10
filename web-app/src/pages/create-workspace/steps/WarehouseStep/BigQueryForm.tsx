import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { WarehousesFormData } from ".";
import { useFormContext } from "react-hook-form";
import { Textarea } from "@/components/ui/shadcn/textarea";

export default function BigQueryForm({ index }: { index: number }) {
  const {
    formState: { errors },
    register,
  } = useFormContext<WarehousesFormData>();

  const configErrors = errors?.warehouses?.[index]?.config as
    | Record<string, { message?: string }>
    | undefined;

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`key-${index}`}>JSON Key</Label>
        <Textarea
          id={`key-${index}`}
          placeholder="JSON key content"
          {...register(`warehouses.${index}.config.key`, {
            required: "JSON Key is required",
          })}
        />
        {configErrors?.key && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.key.message?.toString()}
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          Enter your BigQuery service account key
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`dataset-${index}`}>Dataset</Label>

        <Input
          id={`dataset-${index}`}
          placeholder="your_dataset"
          {...register(`warehouses.${index}.config.dataset`, {
            required: "Dataset is required",
          })}
        />
        {configErrors?.dataset && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.dataset.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`dry_run_limit-${index}`}>Dry Run Limit</Label>
        <Input
          id={`dry_run_limit-${index}`}
          type="number"
          placeholder="1000"
          {...register(`warehouses.${index}.config.dry_run_limit`)}
        />
        {configErrors?.dry_run_limit && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.dry_run_limit.message?.toString()}
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          Limit for dry run queries (optional)
        </p>
      </div>
    </div>
  );
}
