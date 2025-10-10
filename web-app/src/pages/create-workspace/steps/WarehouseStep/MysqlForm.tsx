import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { WarehousesFormData } from ".";
import { useFormContext } from "react-hook-form";

export interface MysqlFormData {
  host?: string;
  port?: string;
  user?: string;
  password?: string;
  password_var?: string;
  database?: string;
}

export default function MysqlForm({ index }: { index: number }) {
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
        <Label htmlFor={`host-${index}`}>Host</Label>
        <Input
          id={`host-${index}`}
          placeholder="localhost"
          {...register(`warehouses.${index}.config.host`, {
            required: "Host is required",
            pattern: {
              value: /^(https?:\/\/)?([a-zA-Z0-9-]+\.)+[a-zA-Z]{2,}(\/.*)?$/,
              message: "Enter a valid URL or host",
            },
          })}
        />
        {configErrors?.host && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.host.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`port-${index}`}>Port</Label>
        <Input
          id={`port-${index}`}
          placeholder="3306"
          {...register(`warehouses.${index}.config.port`, {
            required: "Port is required",
            pattern: {
              value: /^\d+$/,
              message: "Enter a valid port number",
            },
          })}
        />
        {configErrors?.port && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.port.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`user-${index}`}>User</Label>
        <Input
          id={`user-${index}`}
          placeholder="root"
          {...register(`warehouses.${index}.config.username`, {
            required: "User is required",
            pattern: {
              value: /^\w+$/,
              message: "Enter a valid username",
            },
          })}
        />
        {configErrors?.username && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.username.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`password-${index}`}>Password</Label>
        <Input
          id={`password-${index}`}
          type="password"
          placeholder="••••••••"
          {...register(`warehouses.${index}.config.password`, {
            required: "Password is required",
          })}
        />
        {configErrors?.password && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.password.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`database-${index}`}>Database</Label>
        <Input
          id={`database-${index}`}
          placeholder="mysql"
          {...register(`warehouses.${index}.config.database`, {
            required: "Database is required",
            pattern: {
              value: /^\w+$/,
              message: "Enter a valid database name",
            },
          })}
        />
        {configErrors?.database && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.database.message?.toString()}
          </p>
        )}
      </div>
    </div>
  );
}
