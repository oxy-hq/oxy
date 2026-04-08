import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { WarehousesFormData } from "@/types/database";

interface Props {
  index: number;
}

export default function BigQueryForm({ index }: Props) {
  const { register } = useFormContext<WarehousesFormData>();

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label>Service Account Key (JSON)</Label>
        <Textarea
          placeholder='{"type": "service_account", ...}'
          rows={6}
          {...register(`warehouses.${index}.config.key` as never)}
        />
      </div>
      <div className='space-y-2'>
        <Label>Dataset</Label>
        <Input
          placeholder='my_dataset'
          {...register(`warehouses.${index}.config.dataset` as never)}
        />
      </div>
      <div className='space-y-2'>
        <Label>Dry Run Limit</Label>
        <Input
          type='number'
          placeholder='1000'
          {...register(`warehouses.${index}.config.dry_run_limit` as never, {
            valueAsNumber: true
          })}
        />
      </div>
    </div>
  );
}
