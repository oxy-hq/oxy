import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { WarehousesFormData } from "@/types/database";

interface Props {
  index: number;
}

export default function MysqlForm({ index }: Props) {
  const { register } = useFormContext<WarehousesFormData>();

  return (
    <div className='space-y-4'>
      <div className='grid grid-cols-2 gap-4'>
        <div className='space-y-2'>
          <Label>Host</Label>
          <Input
            placeholder='localhost'
            {...register(`warehouses.${index}.config.host` as never)}
          />
        </div>
        <div className='space-y-2'>
          <Label>Port</Label>
          <Input placeholder='3306' {...register(`warehouses.${index}.config.port` as never)} />
        </div>
      </div>
      <div className='space-y-2'>
        <Label>Database</Label>
        <Input placeholder='mydb' {...register(`warehouses.${index}.config.database` as never)} />
      </div>
      <div className='grid grid-cols-2 gap-4'>
        <div className='space-y-2'>
          <Label>User</Label>
          <Input placeholder='root' {...register(`warehouses.${index}.config.user` as never)} />
        </div>
        <div className='space-y-2'>
          <Label>Password</Label>
          <Input
            type='password'
            placeholder='password'
            {...register(`warehouses.${index}.config.password` as never)}
          />
        </div>
      </div>
      <div className='space-y-2'>
        <Label>Password Secret Variable</Label>
        <Input
          placeholder='MY_MYSQL_PASSWORD'
          {...register(`warehouses.${index}.config.password_var` as never)}
        />
      </div>
    </div>
  );
}
