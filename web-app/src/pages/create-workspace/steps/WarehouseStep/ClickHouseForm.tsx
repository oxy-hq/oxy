import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { WarehousesFormData } from ".";
import { useFormContext } from "react-hook-form";

export default function ClickHouseForm({ index }: { index: number }) {
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
          placeholder="http://localhost:8123"
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
        <Label htmlFor={`username-${index}`}>User</Label>
        <Input
          id={`username-${index}`}
          placeholder="default"
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
          placeholder="default"
          {...register(`warehouses.${index}.config.database`, {
            required: "Database is required",
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
